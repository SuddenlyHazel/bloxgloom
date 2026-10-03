//! Bounded, disposable canonical bundle bytes. Verification remains mandatory.
use super::*;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

pub(super) struct Cache {
    root: PathBuf,
}

impl Cache {
    pub(super) fn default() -> Option<Self> {
        crate::config::Config::package_cache_path().map(|root| Self { root })
    }

    fn path(&self, identity: BundleIdentity) -> PathBuf {
        self.root.join(format!(
            "runtime-{}-{}.bundle",
            identity.client_runtime,
            identity.key.cache_name()
        ))
    }

    pub(super) fn load(&self, identity: BundleIdentity) -> io::Result<Option<Arc<ClientBundle>>> {
        identity.validate()?;
        identity.require_supported_runtime()?;
        let path = self.path(identity);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.len() != u64::from(identity.total_len) {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
        let _memory = super::memory::reserve(identity.total_len as usize)?;
        let mut bytes = Vec::with_capacity(identity.total_len as usize);
        open_read(&path)?
            .take(u64::from(identity.total_len) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() != identity.total_len as usize {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
        match super::recovery::decode(&super::CACHE, || {
            ClientBundle::decode_verify(&bytes, identity.key)
        }) {
            Ok(bundle) => Ok(Some(Arc::new(bundle))),
            Err(error) if error.is_bundle_residency_exhausted() => Err(io::Error::other(error)),
            Err(error) => {
                tracing::warn!(%error, cache = %path.display(), "discarding invalid package cache entry");
                let _ = fs::remove_file(path);
                Ok(None)
            }
        }
    }

    pub(super) fn store(&self, identity: BundleIdentity, bundle: &ClientBundle) -> io::Result<()> {
        identity.validate()?;
        identity.require_supported_runtime()?;
        if bundle.cache_key() != identity.key || bundle.bytes().len() != identity.total_len as usize
        {
            return Err(invalid("package cache identity mismatch"));
        }
        fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("writer.lock"))?;
        // Separate processes share the same capacity bound. Cache contention is
        // optional work: skip this write rather than delaying a successful join.
        lock.try_lock().map_err(io::Error::other)?;
        let path = self.path(identity);
        if fs::symlink_metadata(&path)
            .is_ok_and(|m| m.is_file() && m.len() == u64::from(identity.total_len))
        {
            return Ok(());
        }
        self.prune(u64::from(identity.total_len), MAX_ENTRIES - 1)?;
        let temp = self.root.join(format!(
            "pending-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(bundle.bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    fn prune(&self, incoming: u64, keep: usize) -> io::Result<()> {
        let mut entries = Vec::new();
        let mut total = incoming;
        for (index, entry) in fs::read_dir(&self.root)?.enumerate() {
            if index >= 4096 {
                return Err(io::Error::other(
                    "package cache directory entry limit exceeded",
                ));
            }
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_file() {
                continue;
            }
            if name.starts_with("pending-") && name.ends_with(".tmp") {
                // The exclusive writer lock means every old temporary is an
                // interrupted write, never another active cache writer.
                fs::remove_file(entry.path())?;
            } else if cache_name(name) {
                total = total.saturating_add(metadata.len());
                entries.push((metadata.modified()?, entry.path(), metadata.len()));
            }
        }
        entries.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut count = entries.len();
        for (_, path, len) in entries {
            if count <= keep && total <= MAX_BYTES {
                break;
            }
            fs::remove_file(path)?;
            count -= 1;
            total = total.saturating_sub(len);
        }
        Ok(())
    }
}

fn cache_name(name: &str) -> bool {
    let Some((runtime, hash)) = name
        .strip_prefix("runtime-")
        .and_then(|s| s.split_once("-client-v7-sha256-"))
    else {
        return false;
    };
    !runtime.is_empty()
        && runtime.bytes().all(|b| b.is_ascii_digit())
        && hash
            .strip_suffix(".bundle")
            .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn open_read(path: &std::path::Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    options.open(path)
}

#[cfg(test)]
mod tests;
