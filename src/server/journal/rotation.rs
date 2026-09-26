//! Immutable compacted bases and the atomic manifest that selects a generation.

use super::{Reader, StateKey, crc32, invalid_data, invalid_data_owned, sync_parent, validate_key};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST_MAGIC: &[u8; 4] = b"BGWM";
const BASE_MAGIC: &[u8; 4] = b"BGWB";
const TAIL_MAGIC: &[u8; 4] = b"BGWT";
const FORMAT_VERSION: u16 = 1;
const BASE_FORMAT_VERSION_LEGACY: u16 = 1;
const BASE_FORMAT_VERSION: u16 = 2;
const MANIFEST_MAX_BYTES: usize = 4096;
/// Separate fail-closed bound for one materialized latest-state base; this is
/// independent of the 256 MiB append-tail cap and prevents unbounded file reads.
const BASE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
const TAIL_HEADER_LEN: usize = 4 + 2 + 8 + 8 + 4;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(super) struct Manifest {
    pub(super) generation: u64,
    pub(super) cut_sequence: u64,
    pub(super) next_transaction_id: u128,
    pub(super) base_name: String,
    pub(super) tail_name: String,
}

pub(super) struct Base {
    pub(super) generation: u64,
    pub(super) cut_sequence: u64,
    pub(super) next_transaction_id: u128,
    pub(super) drop_owner_set_closed: bool,
    pub(super) values: HashMap<StateKey, Vec<u8>>,
}

pub(super) struct SwitchedGeneration {
    pub(super) file: File,
    pub(super) manifest: Manifest,
    pub(super) old_files: Vec<PathBuf>,
    pub(super) tail_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(super) enum CrashPoint {
    BasePartial,
    BaseTempSynced,
    BaseInstalled,
    TailTempSynced,
    TailInstalled,
    ManifestTempSynced,
    ManifestInstalled,
}

pub(super) fn manifest_path(wal_path: &Path) -> io::Result<PathBuf> {
    suffix_path(wal_path, ".manifest")
}

pub(super) const fn tail_header_len() -> usize {
    TAIL_HEADER_LEN
}

pub(super) fn read_manifest(wal_path: &Path) -> io::Result<Manifest> {
    let path = manifest_path(wal_path)?;
    let bytes = read_bounded(&path, MANIFEST_MAX_BYTES as u64)?;
    if bytes.len() < 4 + 2 + 8 + 8 + 16 + 2 + 2 + 4 {
        return Err(invalid_data("truncated journal generation manifest"));
    }
    check_checksum(&bytes, "journal generation manifest")?;
    let body_len = bytes.len() - 4;
    let mut reader = Reader::new(&bytes[..body_len]);
    if reader.take(4)? != MANIFEST_MAGIC || reader.u16()? != FORMAT_VERSION {
        return Err(invalid_data(
            "invalid or unsupported journal generation manifest",
        ));
    }
    let generation = reader.u64()?;
    let cut_sequence = reader.u64()?;
    let next_transaction_id = reader.u128()?;
    let base_len = reader.u16()? as usize;
    let tail_len = reader.u16()? as usize;
    let base_name = decode_name(reader.take(base_len)?)?;
    let tail_name = decode_name(reader.take(tail_len)?)?;
    if !reader.is_empty() || generation == 0 || next_transaction_id == 0 {
        return Err(invalid_data("invalid journal generation manifest fields"));
    }
    validate_generation_name(&base_name, "base", generation)?;
    validate_generation_name(&tail_name, "tail", generation)?;
    Ok(Manifest {
        generation,
        cut_sequence,
        next_transaction_id,
        base_name,
        tail_name,
    })
}

pub(super) fn base_path(wal_path: &Path, manifest: &Manifest) -> io::Result<PathBuf> {
    named_sibling(wal_path, &manifest.base_name)
}

pub(super) fn tail_path(wal_path: &Path, manifest: &Manifest) -> io::Result<PathBuf> {
    named_sibling(wal_path, &manifest.tail_name)
}

pub(super) fn read_base(wal_path: &Path, manifest: &Manifest) -> io::Result<Base> {
    let path = base_path(wal_path, manifest)?;
    let bytes = read_bounded(&path, BASE_MAX_BYTES).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            invalid_data("journal manifest references a missing base generation")
        } else {
            error
        }
    })?;
    if bytes.len() < 4 + 2 + 8 + 8 + 16 + 8 + 4 {
        return Err(invalid_data("truncated journal generation base"));
    }
    check_checksum(&bytes, "journal generation base")?;
    let body_len = bytes.len() - 4;
    let mut reader = Reader::new(&bytes[..body_len]);
    if reader.take(4)? != BASE_MAGIC {
        return Err(invalid_data(
            "invalid or unsupported journal generation base",
        ));
    }
    let base_format = reader.u16()?;
    let generation = reader.u64()?;
    let cut_sequence = reader.u64()?;
    let next_transaction_id = reader.u128()?;
    let drop_owner_set_closed = match base_format {
        BASE_FORMAT_VERSION_LEGACY => false,
        BASE_FORMAT_VERSION => match reader.u8()? {
            0 => false,
            1 => true,
            _ => {
                return Err(invalid_data(
                    "invalid drop owner-set marker in journal base",
                ));
            }
        },
        _ => {
            return Err(invalid_data(
                "invalid or unsupported journal generation base",
            ));
        }
    };
    let count = usize::try_from(reader.u64()?)
        .map_err(|_| invalid_data("journal generation base key count overflow"))?;
    if generation != manifest.generation
        || cut_sequence != manifest.cut_sequence
        || next_transaction_id != manifest.next_transaction_id
        || next_transaction_id == 0
    {
        return Err(invalid_data(
            "journal generation base does not match its manifest",
        ));
    }

    let mut values = HashMap::new();
    for _ in 0..count {
        let domain_len = reader.u8()? as usize;
        let domain = std::str::from_utf8(reader.take(domain_len)?)
            .map_err(|_| invalid_data("journal base key domain is not UTF-8"))?
            .to_owned();
        let key_len = reader.u32()? as usize;
        let value_len = reader.u32()? as usize;
        let key = StateKey {
            domain,
            bytes: reader.take(key_len)?.to_vec(),
        };
        validate_key(&key)
            .map_err(|error| invalid_data_owned(format!("invalid key in journal base: {error}")))?;
        if value_len > super::MAX_RECORD_BYTES {
            return Err(invalid_data("journal base value exceeds size limit"));
        }
        let value = reader.take(value_len)?.to_vec();
        if values.insert(key, value).is_some() {
            return Err(invalid_data("duplicate key in journal generation base"));
        }
    }
    if !reader.is_empty() {
        return Err(invalid_data("trailing bytes in journal generation base"));
    }
    Ok(Base {
        generation,
        cut_sequence,
        next_transaction_id,
        drop_owner_set_closed,
        values,
    })
}

pub(super) fn open_tail(wal_path: &Path, manifest: &Manifest) -> io::Result<(File, u64)> {
    let path = tail_path(wal_path, manifest)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                invalid_data("journal manifest references a missing tail generation")
            } else {
                error
            }
        })?;
    let length = file.metadata()?.len();
    if length < TAIL_HEADER_LEN as u64 {
        return Err(invalid_data("truncated journal generation tail header"));
    }
    let mut header = [0u8; TAIL_HEADER_LEN];
    file.read_exact(&mut header)?;
    let checksum = u32::from_le_bytes(header[TAIL_HEADER_LEN - 4..].try_into().unwrap());
    if &header[..4] != TAIL_MAGIC
        || u16::from_le_bytes(header[4..6].try_into().unwrap()) != FORMAT_VERSION
        || u64::from_le_bytes(header[6..14].try_into().unwrap()) != manifest.generation
        || u64::from_le_bytes(header[14..22].try_into().unwrap()) != manifest.cut_sequence
        || checksum != crc32(&header[..TAIL_HEADER_LEN - 4])
    {
        return Err(invalid_data("invalid journal generation tail header"));
    }
    Ok((file, length))
}

pub(super) fn rotate(
    wal_path: &Path,
    old_manifest: Option<&Manifest>,
    cut_sequence: u64,
    next_transaction_id: u128,
    values: &BTreeMap<StateKey, Vec<u8>>,
    drop_owner_set_closed: bool,
) -> io::Result<SwitchedGeneration> {
    rotate_inner(
        wal_path,
        old_manifest,
        cut_sequence,
        next_transaction_id,
        values,
        drop_owner_set_closed,
        #[cfg(test)]
        None,
    )
}

#[cfg(test)]
pub(super) fn rotate_crashing_at(
    wal_path: &Path,
    old_manifest: Option<&Manifest>,
    cut_sequence: u64,
    next_transaction_id: u128,
    values: &BTreeMap<StateKey, Vec<u8>>,
    drop_owner_set_closed: bool,
    crash_at: CrashPoint,
) -> io::Result<SwitchedGeneration> {
    rotate_inner(
        wal_path,
        old_manifest,
        cut_sequence,
        next_transaction_id,
        values,
        drop_owner_set_closed,
        Some(crash_at),
    )
}

fn rotate_inner(
    wal_path: &Path,
    old_manifest: Option<&Manifest>,
    cut_sequence: u64,
    next_transaction_id: u128,
    values: &BTreeMap<StateKey, Vec<u8>>,
    drop_owner_set_closed: bool,
    #[cfg(test)] crash_at: Option<CrashPoint>,
) -> io::Result<SwitchedGeneration> {
    if next_transaction_id == 0 {
        return Err(invalid_data("journal transaction ID space exhausted"));
    }
    let parent = parent_dir(wal_path);
    fs::create_dir_all(parent)?;
    let prefix = wal_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_data("journal path must have a UTF-8 file name to rotate"))?;
    let mut generation = old_manifest
        .map_or(Some(1), |manifest| manifest.generation.checked_add(1))
        .ok_or_else(|| invalid_data("journal generation exhausted"))?;
    let (base_name, tail_name) = loop {
        let base_name = format!("{prefix}.base.{generation}");
        let tail_name = format!("{prefix}.tail.{generation}");
        if !named_sibling(wal_path, &base_name)?.exists()
            && !named_sibling(wal_path, &tail_name)?.exists()
        {
            break (base_name, tail_name);
        }
        generation = generation
            .checked_add(1)
            .ok_or_else(|| invalid_data("journal generation exhausted"))?;
    };
    let manifest = Manifest {
        generation,
        cut_sequence,
        next_transaction_id,
        base_name,
        tail_name,
    };
    let base_final = base_path(wal_path, &manifest)?;
    let tail_final = tail_path(wal_path, &manifest)?;
    let manifest_final = manifest_path(wal_path)?;

    let (base_temp, base_file) = create_temp(&base_final)?;
    let mut base_file = base_file;
    write_base(
        &mut base_file,
        &manifest,
        values,
        drop_owner_set_closed,
        #[cfg(test)]
        crash_at,
    )?;
    base_file.sync_all()?;
    drop(base_file);
    #[cfg(test)]
    crash(crash_at, CrashPoint::BaseTempSynced)?;

    let (tail_temp, mut tail_file) = create_temp(&tail_final)?;
    let tail_header = encode_tail_header(manifest.generation, cut_sequence);
    tail_file.write_all(&tail_header)?;
    tail_file.sync_all()?;
    drop(tail_file);
    #[cfg(test)]
    crash(crash_at, CrashPoint::TailTempSynced)?;

    fs::rename(&base_temp, &base_final)?;
    fs::rename(&tail_temp, &tail_final)?;
    sync_parent(wal_path)?;
    #[cfg(test)]
    crash(crash_at, CrashPoint::BaseInstalled)?;
    #[cfg(test)]
    crash(crash_at, CrashPoint::TailInstalled)?;

    let mut active_tail = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&tail_final)?;
    active_tail.seek(std::io::SeekFrom::End(0))?;

    let manifest_bytes = encode_manifest(&manifest)?;
    let (manifest_temp, mut manifest_file) = create_temp(&manifest_final)?;
    manifest_file.write_all(&manifest_bytes)?;
    manifest_file.sync_all()?;
    drop(manifest_file);
    #[cfg(test)]
    crash(crash_at, CrashPoint::ManifestTempSynced)?;

    fs::rename(&manifest_temp, &manifest_final)?;
    #[cfg(test)]
    crash(crash_at, CrashPoint::ManifestInstalled)?;
    sync_parent(wal_path)?;

    let old_files = if let Some(old) = old_manifest {
        vec![base_path(wal_path, old)?, tail_path(wal_path, old)?]
    } else {
        vec![wal_path.to_owned()]
    };
    let tail_bytes = active_tail.metadata()?.len();
    Ok(SwitchedGeneration {
        file: active_tail,
        manifest,
        old_files,
        tail_bytes,
    })
}

pub(super) fn cleanup_old_files(wal_path: &Path, files: &[PathBuf]) {
    let mut changed = false;
    for path in files {
        changed |= fs::remove_file(path).is_ok();
    }
    if changed {
        let _ = sync_parent(wal_path);
    }
}

fn write_base(
    output: &mut impl Write,
    manifest: &Manifest,
    values: &BTreeMap<StateKey, Vec<u8>>,
    drop_owner_set_closed: bool,
    #[cfg(test)] crash_at: Option<CrashPoint>,
) -> io::Result<()> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(BASE_MAGIC);
    bytes.extend_from_slice(&BASE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&manifest.generation.to_le_bytes());
    bytes.extend_from_slice(&manifest.cut_sequence.to_le_bytes());
    bytes.extend_from_slice(&manifest.next_transaction_id.to_le_bytes());
    bytes.push(u8::from(drop_owner_set_closed));
    bytes.extend_from_slice(&(values.len() as u64).to_le_bytes());
    let header = bytes;
    let entries = values.iter().map(|(key, value)| {
        validate_key(key)?;
        if value.len() > super::MAX_RECORD_BYTES {
            return Err(invalid_data("journal base value exceeds size limit"));
        }
        // One schema-bounded value, never a whole-generation buffer. The
        // live ordered latest map is frozen by the WAL rotation barrier.
        let mut bytes = Vec::with_capacity(9 + key.domain.len() + key.bytes.len() + value.len());
        bytes.push(key.domain.len() as u8);
        bytes.extend_from_slice(key.domain.as_bytes());
        bytes.extend_from_slice(&(key.bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&key.bytes);
        bytes.extend_from_slice(value);
        Ok(bytes)
    });
    crate::server::checkpoint_stream::write_frame(
        output,
        BASE_MAX_BYTES as usize,
        std::iter::once(Ok(header)).chain(entries),
        crate::server::checkpoint_stream::TURN_ENTRIES,
        |_| {
            #[cfg(test)]
            crash(crash_at, CrashPoint::BasePartial)?;
            // The WAL worker is deliberately unavailable for append until
            // this generation is durably selected. Admission is already shut.
            std::thread::yield_now();
            Ok(())
        },
    )
}

fn encode_manifest(manifest: &Manifest) -> io::Result<Vec<u8>> {
    validate_generation_name(&manifest.base_name, "base", manifest.generation)?;
    validate_generation_name(&manifest.tail_name, "tail", manifest.generation)?;
    let base = manifest.base_name.as_bytes();
    let tail = manifest.tail_name.as_bytes();
    let base_len =
        u16::try_from(base.len()).map_err(|_| invalid_data("base file name too long"))?;
    let tail_len =
        u16::try_from(tail.len()).map_err(|_| invalid_data("tail file name too long"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MANIFEST_MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&manifest.generation.to_le_bytes());
    bytes.extend_from_slice(&manifest.cut_sequence.to_le_bytes());
    bytes.extend_from_slice(&manifest.next_transaction_id.to_le_bytes());
    bytes.extend_from_slice(&base_len.to_le_bytes());
    bytes.extend_from_slice(&tail_len.to_le_bytes());
    bytes.extend_from_slice(base);
    bytes.extend_from_slice(tail);
    let checksum = crc32(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    Ok(bytes)
}

fn encode_tail_header(generation: u64, cut_sequence: u64) -> [u8; TAIL_HEADER_LEN] {
    let mut header = [0u8; TAIL_HEADER_LEN];
    header[..4].copy_from_slice(TAIL_MAGIC);
    header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    header[6..14].copy_from_slice(&generation.to_le_bytes());
    header[14..22].copy_from_slice(&cut_sequence.to_le_bytes());
    let checksum = crc32(&header[..TAIL_HEADER_LEN - 4]);
    header[TAIL_HEADER_LEN - 4..].copy_from_slice(&checksum.to_le_bytes());
    header
}

fn create_temp(final_path: &Path) -> io::Result<(PathBuf, File)> {
    let mut attempt = 0u32;
    loop {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let mut name = final_path
            .file_name()
            .ok_or_else(|| invalid_data("journal path has no file name"))?
            .to_os_string();
        name.push(format!(".tmp.{}.{}.{}", std::process::id(), id, attempt));
        let path = parent_dir(final_path).join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                attempt = attempt
                    .checked_add(1)
                    .ok_or_else(|| invalid_data("journal temporary-name space exhausted"))?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    if length > limit {
        return Err(invalid_data("journal generation file exceeds size limit"));
    }
    let capacity = usize::try_from(length)
        .map_err(|_| invalid_data("journal generation file length overflow"))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)?;
    if bytes.len() != capacity {
        return Err(invalid_data(
            "journal generation file changed while reading",
        ));
    }
    Ok(bytes)
}

fn check_checksum(bytes: &[u8], label: &'static str) -> io::Result<()> {
    let checksum_offset = bytes.len() - 4;
    let expected = u32::from_le_bytes(bytes[checksum_offset..].try_into().unwrap());
    if expected != crc32(&bytes[..checksum_offset]) {
        return Err(invalid_data_owned(format!("{label} checksum mismatch")));
    }
    Ok(())
}

fn decode_name(bytes: &[u8]) -> io::Result<String> {
    let name = std::str::from_utf8(bytes)
        .map_err(|_| invalid_data("journal generation file name is not UTF-8"))?
        .to_owned();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(invalid_data("invalid journal generation file name"));
    }
    Ok(name)
}

fn validate_generation_name(name: &str, kind: &str, generation: u64) -> io::Result<()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || !name.ends_with(&format!(".{kind}.{generation}"))
    {
        return Err(invalid_data("invalid journal generation file reference"));
    }
    Ok(())
}

fn suffix_path(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let mut name = path
        .file_name()
        .ok_or_else(|| invalid_data("journal path has no file name"))?
        .to_os_string();
    name.push(suffix);
    Ok(parent_dir(path).join(name))
}

fn named_sibling(path: &Path, name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(invalid_data("invalid journal generation file name"));
    }
    Ok(parent_dir(path).join(name))
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
fn crash(point: Option<CrashPoint>, wanted: CrashPoint) -> io::Result<()> {
    if point == Some(wanted) {
        Err(io::Error::other(format!("injected crash at {wanted:?}")))
    } else {
        Ok(())
    }
}
