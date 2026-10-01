//! Profile-owned cosmetic replacement. The simulation thread is the sole writer;
//! an atomic per-profile file is the commit point, before transient entity publish.
//! This does not enter the inventory/world WAL: no items or world edits are coupled.
use super::State;
use crate::appearance::{AppearanceState, CharacterRecipe, MAX_APPEARANCE_BYTES};
use crate::content::Catalog;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_LEN: usize = 4 + 16 + 1 + MAX_APPEARANCE_BYTES + 4;
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

    pub(super) fn load(&self, profile: u128, catalog: &Catalog) -> io::Result<AppearanceState> {
        let file = match File::open(self.path(profile)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AppearanceState::default());
            }
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::with_capacity(MAX_LEN + 1);
        file.take((MAX_LEN + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.starts_with(b"BGA2") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "profile appearance is from an incompatible older world; use a new world directory (old save left unchanged)",
            ));
        }
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or unsupported profile appearance",
            )
        };
        if !(29..=MAX_LEN).contains(&bytes.len())
            || &bytes[..4] != b"BGA3"
            || u128::from_le_bytes(bytes[4..20].try_into().unwrap()) != profile
            || usize::from(bytes[20]) + 25 != bytes.len()
        {
            return Err(invalid());
        }
        let end = bytes.len() - 4;
        if u32::from_le_bytes(bytes[end..].try_into().unwrap()) != checksum(&bytes[..end]) {
            return Err(invalid());
        }
        let appearance = AppearanceState::decode(&bytes[21..end]).ok_or_else(invalid)?;
        if !catalog.valid_appearance_state(appearance) {
            return Err(invalid());
        }
        Ok(appearance)
    }

    fn save(&self, profile: u128, appearance: AppearanceState) -> io::Result<()> {
        let payload = appearance.encode();
        let mut bytes = b"BGA3".to_vec();
        bytes.extend(profile.to_le_bytes());
        bytes.push(payload.len() as u8);
        bytes.extend(payload);
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
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    appearance.palettes = palettes;
    replace(state, session, appearance)
}

pub(super) fn select_character(
    state: &mut State,
    session: u64,
    recipe: Option<CharacterRecipe>,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    appearance.character = recipe;
    replace(state, session, appearance)
}

fn replace(state: &mut State, session: u64, appearance: AppearanceState) -> io::Result<()> {
    if state.durability.failed {
        return Err(io::Error::other("server durability failed"));
    }
    if !state.world.catalog().valid_appearance_state(appearance) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unregistered appearance selection",
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
