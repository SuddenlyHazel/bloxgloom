//! Per-chunk drop checkpoint shards: framing, validation, and load rules.

use super::super::shards::{
    allocator_path, decode_allocator, decode_shard, encode_allocator, encode_shard,
    load_sharded, shard_dir_for_drops_file, shard_path, write_allocator_snapshot,
    write_shard_snapshot, SHARD_HEADER,
};
use super::*;
use std::time::Duration;

fn chunk_entries_sorted(drops: &Drops, chunk: crate::world::ChunkKey) -> Vec<&Entry> {
    let mut entries: Vec<&Entry> = drops
        .chunk_members
        .get(&chunk)
        .map(|members| {
            members
                .iter()
                .filter_map(|id| drops.entries.get(id))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|entry| entry.id);
    entries
}

#[test]
fn shard_round_trip_preserves_members() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [1.0, 2.0, 3.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        2,
        [40.0, 2.0, 3.0],
        2,
        5,
        Duration::ZERO,
        Duration::ZERO,
    );
    let chunk = super::super::chunk_of([1.0, 2.0, 3.0]);
    let entries = chunk_entries_sorted(&drops, chunk);
    assert_eq!(entries.len(), 1);
    let bytes = encode_shard(chunk, &entries).unwrap();
    let decoded = decode_shard(&bytes, drops.catalog.as_ref()).unwrap();
    assert_eq!(decoded.chunk, chunk);
    assert_eq!(decoded.drops.len(), 1);
    assert_eq!(decoded.drops[0].0, 1);
    assert_eq!(decoded.drops[0].1, [1.0, 2.0, 3.0]);
    assert_eq!(decoded.drops[0].2.stack.count, 10);
}

#[test]
fn shard_decode_rejects_corruption() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [1.0, 2.0, 3.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    let chunk = super::super::chunk_of([1.0, 2.0, 3.0]);
    let entries = chunk_entries_sorted(&drops, chunk);
    let good = encode_shard(chunk, &entries).unwrap();
    let catalog = drops.catalog.clone();

    let mut bad_magic = good.clone();
    bad_magic[0] ^= 0xff;
    assert!(decode_shard(&bad_magic, &catalog).is_err());

    let mut bad_checksum = good.clone();
    let last = bad_checksum.len() - 1;
    bad_checksum[last] ^= 0xff;
    assert!(decode_shard(&bad_checksum, &catalog).is_err());

    assert!(decode_shard(&good[..SHARD_HEADER], &catalog).is_err());
    assert!(decode_shard(&good[..good.len() - 5], &catalog).is_err());

    // A member that belongs to another chunk fails closed.
    let other = super::super::chunk_of([400.0, 2.0, 3.0]);
    assert_ne!(other, chunk);
    assert!(decode_shard(&encode_shard(other, &entries).unwrap(), &catalog).is_err());
}

#[test]
fn allocator_checkpoint_round_trip_and_rejects_corruption() {
    let bytes = encode_allocator(41, 7);
    assert_eq!(decode_allocator(&bytes).unwrap(), (41, 7));

    let mut bad = bytes.clone();
    bad[0] ^= 0xff;
    assert!(decode_allocator(&bad).is_err());
    let mut bad_sum = bytes.clone();
    let last = bad_sum.len() - 1;
    bad_sum[last] ^= 0xff;
    assert!(decode_allocator(&bad_sum).is_err());
    assert!(decode_allocator(&bytes[..bytes.len() - 1]).is_err());
    assert!(decode_allocator(&encode_allocator(0, 7)).is_err());
}

#[test]
fn load_sharded_returns_none_without_markers() {
    let root = temp_root("bloxgloom-drop-shard-absent");
    std::fs::create_dir_all(&root).unwrap();
    let catalog = crate::content::catalog().clone();
    assert!(load_sharded(&root.join("drops.d"), &catalog).is_none());
    // An empty shard directory is not a marker either: legacy loads instead.
    std::fs::create_dir_all(root.join("drops.d")).unwrap();
    assert!(load_sharded(&root.join("drops.d"), &catalog).is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_sharded_reads_shards_and_allocator() {
    let root = temp_root("bloxgloom-drop-shard-load");
    std::fs::create_dir_all(&root).unwrap();
    let dir = shard_dir_for_drops_file(&root.join("drops.bin"));
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [1.0, 2.0, 3.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        9,
        [40.0, 2.0, 3.0],
        2,
        3,
        Duration::ZERO,
        Duration::ZERO,
    );
    for (chunk, bytes) in drops.take_dirty_shard_snapshots() {
        write_shard_snapshot(&shard_path(&dir, chunk), &bytes).unwrap();
    }
    write_allocator_snapshot(
        &allocator_path(&dir),
        &encode_allocator(drops.next_id, drops.revision),
    )
    .unwrap();

    let catalog = crate::content::catalog().clone();
    let loaded = load_sharded(&dir, &catalog)
        .expect("shard markers exist")
        .unwrap();
    assert_eq!(loaded.drops.len(), 2);
    assert_eq!(loaded.next_id, drops.next_id);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_sharded_rejects_chunk_name_mismatch() {
    let root = temp_root("bloxgloom-drop-shard-dup");
    std::fs::create_dir_all(&root).unwrap();
    let dir = shard_dir_for_drops_file(&root.join("drops.bin"));
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [1.0, 2.0, 3.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    let chunk_a = super::super::chunk_of([1.0, 2.0, 3.0]);
    let chunk_b = super::super::chunk_of([40.0, 2.0, 3.0]);
    assert_ne!(chunk_a, chunk_b);
    let entries = chunk_entries_sorted(&drops, chunk_a);
    // A shard whose file name disagrees with its inner chunk fails closed.
    write_shard_snapshot(
        &shard_path(&dir, chunk_b),
        &encode_shard(chunk_a, &entries).unwrap(),
    )
    .unwrap();
    write_allocator_snapshot(&allocator_path(&dir), &encode_allocator(2, 0)).unwrap();

    let catalog = crate::content::catalog().clone();
    assert!(load_sharded(&dir, &catalog).unwrap().is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_shard_write_deletes_its_file() {
    let root = temp_root("bloxgloom-drop-shard-empty");
    std::fs::create_dir_all(&root).unwrap();
    let dir = shard_dir_for_drops_file(&root.join("drops.bin"));
    let chunk = super::super::chunk_of([1.0, 2.0, 3.0]);
    let full = encode_shard(chunk, &[]).unwrap();
    // Sanity: an empty member list still frames a valid empty shard.
    let catalog = crate::content::catalog().clone();
    assert_eq!(decode_shard(&full, &catalog).unwrap().drops.len(), 0);
    let path = shard_path(&dir, chunk);
    write_shard_snapshot(&path, &encode_shard(chunk, &[]).unwrap()).unwrap();
    // Deleting a missing file is a no-op success.
    assert!(!path.exists());
    write_allocator_snapshot(&allocator_path(&dir), &encode_allocator(2, 0)).unwrap();
    let loaded = load_sharded(&dir, &catalog).unwrap().unwrap();
    assert!(loaded.drops.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
