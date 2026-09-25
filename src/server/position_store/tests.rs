use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestSave(PathBuf);

impl TestSave {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-position-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestSave {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn position_round_trips_and_is_profile_scoped() {
    let save = TestSave::new();
    let store = PositionStore::new(&save.0).unwrap();
    assert_eq!(store.load(1).unwrap(), None);
    store.save(1, [123.5, 42.25, -70.5]).unwrap();
    store.save(2, [-4.5, 80.0, 9.0]).unwrap();
    assert_eq!(store.load(1).unwrap(), Some([123.5, 42.25, -70.5]));
    assert_eq!(store.load(2).unwrap(), Some([-4.5, 80.0, 9.0]));
    store.save(1, [124.5, 42.25, -70.5]).unwrap();
    assert_eq!(store.load(1).unwrap(), Some([124.5, 42.25, -70.5]));
}

#[test]
fn corrupted_position_is_not_silently_replaced() {
    let save = TestSave::new();
    let store = PositionStore::new(&save.0).unwrap();
    store.save(7, [0.5, 80.0, 0.5]).unwrap();
    let path = store.path(7);
    let mut bytes = fs::read(&path).unwrap();
    bytes[24] ^= 0x80;
    fs::write(path, bytes).unwrap();
    assert_eq!(
        store.load(7).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}
