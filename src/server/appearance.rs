//! Profile-owned cosmetic replacement. The simulation thread is the sole writer;
//! an atomic per-profile file is the commit point, before transient entity publish.
//! This does not enter the inventory/world WAL: no items or world edits are coupled.
use super::State;
use crate::content::Catalog;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const LEN: usize = 4 + 16 + 4 + 4;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct Store {
    root: PathBuf,
}
impl Store {
    pub(super) fn new(world: &Path) -> io::Result<Self> {
        let root = world.join("players");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    fn path(&self, profile: u128) -> PathBuf {
        self.root.join(format!("{profile:032x}.appearance"))
    }

    pub(super) fn load(&self, profile: u128, catalog: &Catalog) -> io::Result<[u8; 4]> {
        let file = match File::open(self.path(profile)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok([0; 4]),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::with_capacity(LEN + 1);
        file.take((LEN + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() != LEN
            || &bytes[..4] != b"BGA1"
            || u128::from_le_bytes(bytes[4..20].try_into().unwrap()) != profile
            || u32::from_le_bytes(bytes[24..28].try_into().unwrap()) != checksum(&bytes[..24])
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid profile appearance",
            ));
        }
        let appearance = bytes[20..24].try_into().unwrap();
        if !catalog.valid_appearance(appearance) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unregistered saved appearance",
            ));
        }
        Ok(appearance)
    }

    fn save(&self, profile: u128, appearance: [u8; 4]) -> io::Result<()> {
        let mut bytes = b"BGA1".to_vec();
        bytes.extend(profile.to_le_bytes());
        bytes.extend(appearance);
        bytes.extend(checksum(&bytes).to_le_bytes());
        let temporary = self.root.join(format!(
            ".{profile:032x}.appearance.{}.{}.tmp",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.path(profile))?;
            File::open(&self.root)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5, |hash: u32, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x01000193)
    })
}

pub(super) fn select(state: &mut State, session: u64, palettes: [u8; 3]) -> io::Result<()> {
    if state.durability.failed {
        return Err(io::Error::other("server durability failed"));
    }
    let appearance = [palettes[0], palettes[1], palettes[2], 0];
    if !state.world.catalog().valid_appearance(appearance) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unregistered appearance palette",
        ));
    }
    let Some(client) = state.clients.get(&session) else {
        return Ok(());
    };
    // Preparation checks revision exhaustion and equality before any I/O. An
    // identical retry is a no-op; no response loss can create another mutation.
    let Some(view) = state
        .player_entities
        .prepare_appearance(session, appearance)
        .map_err(io::Error::other)?
    else {
        return Ok(());
    };
    if let Err(error) = state.appearance_store.save(client.profile, appearance) {
        // Rename may already have committed when directory fsync failed. Never
        // continue with two authorities; recovery reads the whole atomic file.
        state.durability.failed = true;
        return Err(error);
    }
    let delta = state.player_entities.apply_appearance(session, view);
    if let Err(error) = state.queue_player_entity_deltas(vec![delta]) {
        state.durability.failed = true;
        return Err(error);
    }
    Ok(())
}
