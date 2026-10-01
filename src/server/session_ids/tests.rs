use super::*;
fn directory(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "bloxgloom-session-generation-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
#[test]
fn session_boot_ranges_preserve_first_ids_and_never_reuse_a_durable_player_reference() {
    let path = directory("restart");
    let first = reserve(&path).unwrap();
    assert_eq!(first, 1);
    assert_eq!(next_after(first).unwrap(), 2);
    let next = reserve(&path).unwrap();
    assert_eq!(next, (1 << 32) + 1);
    let previous_target = crate::server::entities::EntityId::for_player_session(first).unwrap();
    let restarted_target = crate::server::entities::EntityId::for_player_session(next).unwrap();
    assert_ne!(
        previous_target, restarted_target,
        "persisted impact/source must not resolve to the next boot's first session"
    );
    assert_eq!(
        reserve(&path).unwrap(),
        (2 << 32) + 1,
        "even an unused boot range is never reclaimed"
    );
    let end = u64::from(u32::MAX);
    assert_eq!(next_after(end).unwrap(), 1 << 32);
    assert_eq!(
        next_after(1 << 32).unwrap_err().kind(),
        ErrorKind::QuotaExceeded
    );
    assert_eq!(
        next_after(1 << 63).unwrap_err().kind(),
        ErrorKind::QuotaExceeded
    );
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn session_reservations_are_serialized_and_corrupt_or_exhausted_counters_are_rejected() {
    let path = directory("concurrent");
    fs::create_dir_all(&path).unwrap();
    let handles = (0..8)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || reserve(&path).unwrap())
        })
        .collect::<Vec<_>>();
    let mut ids = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(
        ids,
        (0..8u64)
            .map(|generation| (generation << 32) | 1)
            .collect::<Vec<_>>()
    );
    fs::write(path.join(FILE), b"bad").unwrap();
    assert_eq!(reserve(&path).unwrap_err().kind(), ErrorKind::InvalidData);
    let mut bytes = b"BGS1".to_vec();
    bytes.extend(GENERATIONS.to_le_bytes());
    bytes.extend((!GENERATIONS).to_le_bytes());
    fs::write(path.join(FILE), bytes).unwrap();
    assert_eq!(reserve(&path).unwrap_err().kind(), ErrorKind::QuotaExceeded);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn session_server_restart_installs_a_new_range_before_player_admission() {
    let path = directory("server-restart");
    let state = crate::server::server_state(73, path.clone()).unwrap();
    let previous = state.next_id;
    assert_eq!(previous, 1);
    drop(state);
    let restarted = crate::server::server_state(73, path.clone()).unwrap();
    assert_eq!(restarted.next_id, (1 << 32) + 1);
    assert_ne!(
        crate::server::entities::EntityId::for_player_session(previous),
        crate::server::entities::EntityId::for_player_session(restarted.next_id),
    );
    drop(restarted);
    fs::remove_dir_all(path).unwrap();
}
