use super::*;
use crate::content::{Catalog, EntityTypeId};
use crate::protocol::{
    BlockCellChange, ClientMessage, EntitySnapshotPage, PublicEntityLocation, WorldCommitPart,
};
use crate::world::{AIR, CHUNK_VOLUME, DIRT, STONE};

#[test]
fn builtin_kiln_controls_emit_only_bounded_generic_interactions() {
    use crate::raycast::{Face, Hit};
    let catalog = Catalog::builtins();
    let hit = Hit {
        block: [5, 12, -3],
        adjacent: [5, 12, -2],
        block_id: crate::content::KILN_DEFAULT_STATE,
        distance: 2.0,
        face: Face::PosZ,
    };
    assert!(kiln::is_kiln_hit(hit, &catalog));
    let ClientMessage::EntityInteract {
        action_id,
        target,
        payload,
    } = kiln::interaction(hit, (1u128 << 64) | 2, 4, kiln::KilnCommand::TakeOutput)
    else {
        panic!("kiln control must use the generic interaction wire");
    };
    assert_eq!(action_id, (1u128 << 64) | 2);
    assert_eq!(target, hit.block);
    assert_eq!(payload, [1, 1, 2, 4, 1, 0]);
    let mut ordinary = hit;
    ordinary.block_id = STONE;
    assert!(!kiln::is_kiln_hit(ordinary, &catalog));
}

fn key(x: i32) -> ChunkKey {
    ChunkKey { x, y: 0, z: 0 }
}

fn chunk(key: ChunkKey, version: u64) -> Chunk {
    Chunk::from_blocks(key, version, vec![AIR; CHUNK_VOLUME])
}

fn player(id: u64, revision: u64) -> PublicEntity {
    PublicEntity {
        id,
        entity_type: EntityTypeId(2),
        revision,
        motion_revision: revision,
        location: PublicEntityLocation::Mobile {
            position: [0.5, 2.0, 0.5],
        },
        payload: vec![1, 2, 3, 4],
    }
}

fn snapshot(
    key: ChunkKey,
    epoch: u64,
    entity_revision: u64,
    pages: Vec<Vec<PublicEntity>>,
    catalog: &Catalog,
) -> (WorldSnapshotStart, Vec<EntitySnapshotPage>) {
    let chunk = chunk(key, 0);
    let checksum = snapshot_checksum(&chunk, epoch, entity_revision, &pages, catalog).unwrap();
    let page_count = pages.len() as u16;
    let start = WorldSnapshotStart {
        chunk,
        epoch,
        entity_revision,
        entity_page_count: page_count,
        checksum,
    };
    let pages = pages
        .into_iter()
        .enumerate()
        .map(|(index, entities)| EntitySnapshotPage {
            key,
            epoch,
            entity_revision,
            page_index: index as u16,
            page_count,
            checksum,
            entities,
        })
        .collect();
    (start, pages)
}

fn accept(
    replicas: &mut Replicas,
    message: ServerMessage,
    catalog: &Catalog,
    chunks: &mut HashMap<ChunkKey, Arc<Chunk>>,
) -> Assembly {
    replicas.accept(message, catalog, chunks)
}

#[test]
fn reordered_duplicate_pages_install_once_and_late_pages_cannot_resurrect() {
    let catalog = Catalog::builtins();
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let k = key(0);
    let (start, pages) = snapshot(
        k,
        3,
        7,
        vec![vec![player(1, 1)], vec![player(2, 1)]],
        &catalog,
    );
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(pages[1].clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert!(chunks.is_empty());
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldSnapshotStart(start.clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(pages[1].clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(pages[0].clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Installed(keys) if keys == vec![k]
    ));
    assert_eq!(replicas.entities_in(k).unwrap().len(), 2);
    assert_eq!(replicas.visual_avatars(glam::Vec3::ZERO, None).len(), 2);
    assert_eq!(replicas.visual_avatars(glam::Vec3::ZERO, Some(1)).len(), 1);

    let (fresh, _) = snapshot(k, 4, 8, vec![vec![player(2, 2)]], &catalog);
    let (_, fresh_pages) = snapshot(k, 4, 8, vec![vec![player(2, 2)]], &catalog);
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldSnapshotStart(fresh),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(fresh_pages[0].clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Installed(_)
    ));
    assert_eq!(
        replicas
            .entities_in(k)
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(replicas.visual_avatars(glam::Vec3::ZERO, None).len(), 1);
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(pages[0].clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert_eq!(
        replicas
            .entities_in(k)
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
}

#[test]
fn block_and_entity_changes_wait_for_whole_cross_chunk_commit() {
    let catalog = Catalog::builtins();
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    for k in [key(0), key(1)] {
        let (start, _) = snapshot(k, 1, 0, vec![], &catalog);
        assert!(matches!(
            accept(
                &mut replicas,
                ServerMessage::WorldSnapshotStart(start),
                &catalog,
                &mut chunks
            ),
            Assembly::Installed(_)
        ));
    }
    let first = WorldCommitPart {
        commit_id: 9,
        part_index: 0,
        part_count: 2,
        key: key(0),
        epoch: 1,
        block_from: 0,
        block_to: 1,
        entity_from: 0,
        entity_to: 5,
        blocks: vec![BlockCellChange {
            local: [0, 0, 0],
            block: STONE,
        }],
        entities: vec![PublicEntityChange::Upsert(player(12, 1))],
    };
    let second = WorldCommitPart {
        commit_id: 9,
        part_index: 1,
        part_count: 2,
        key: key(1),
        epoch: 1,
        block_from: 0,
        block_to: 1,
        entity_from: 0,
        entity_to: 0,
        blocks: vec![BlockCellChange {
            local: [1, 0, 0],
            block: DIRT,
        }],
        entities: vec![],
    };
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldCommitPart(second.clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert_eq!(chunks[&key(0)].version, 0);
    assert_eq!(chunks[&key(1)].version, 0);
    assert!(matches!(
        accept(&mut replicas, ServerMessage::WorldCommitPart(first.clone()), &catalog, &mut chunks),
        Assembly::Installed(keys) if keys == vec![key(0), key(1)]
    ));
    assert_eq!(chunks[&key(0)].block([0, 0, 0]), Some(STONE));
    assert_eq!(chunks[&key(1)].block([1, 0, 0]), Some(DIRT));
    assert!(replicas.entities_in(key(0)).unwrap().contains_key(&12));
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldCommitPart(first),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert_eq!(chunks[&key(0)].version, 1);

    let stale = WorldCommitPart {
        commit_id: 10,
        part_index: 0,
        part_count: 1,
        ..second
    };
    assert!(matches!(
        accept(&mut replicas, ServerMessage::WorldCommitPart(stale), &catalog, &mut chunks),
        Assembly::Resync(keys) if keys == vec![key(1)]
    ));
    assert_eq!(chunks[&key(1)].version, 1);
}

#[test]
fn checksum_conflict_and_revision_gap_request_resync_without_partial_install() {
    let catalog = Catalog::builtins();
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let k = key(0);
    let (start, mut pages) = snapshot(k, 1, 0, vec![vec![player(1, 1)]], &catalog);
    pages[0].checksum ^= 1;
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldSnapshotStart(start),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert!(matches!(
        accept(&mut replicas, ServerMessage::EntitySnapshotPage(pages[0].clone()), &catalog, &mut chunks),
        Assembly::Resync(keys) if keys == vec![k]
    ));
    assert!(!chunks.contains_key(&k));

    let (start, _) = snapshot(k, 2, 0, vec![], &catalog);
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldSnapshotStart(start),
            &catalog,
            &mut chunks
        ),
        Assembly::Installed(_)
    ));
    let stale = WorldCommitPart {
        commit_id: 1,
        part_index: 0,
        part_count: 1,
        key: k,
        epoch: 2,
        block_from: 1,
        block_to: 2,
        entity_from: 0,
        entity_to: 0,
        blocks: vec![BlockCellChange {
            local: [0, 0, 0],
            block: STONE,
        }],
        entities: vec![],
    };
    assert!(matches!(
        accept(&mut replicas, ServerMessage::WorldCommitPart(stale), &catalog, &mut chunks),
        Assembly::Resync(keys) if keys == vec![k]
    ));
    assert_eq!(chunks[&k].version, 0);
}

#[test]
fn cross_chunk_player_transfer_changes_avatar_only_after_full_group() {
    let catalog = Catalog::builtins();
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let (start, pages) = snapshot(key(0), 1, 1, vec![vec![player(41, 1)]], &catalog);
    accept(
        &mut replicas,
        ServerMessage::WorldSnapshotStart(start),
        &catalog,
        &mut chunks,
    );
    accept(
        &mut replicas,
        ServerMessage::EntitySnapshotPage(pages[0].clone()),
        &catalog,
        &mut chunks,
    );
    let (start, _) = snapshot(key(1), 1, 1, vec![], &catalog);
    accept(
        &mut replicas,
        ServerMessage::WorldSnapshotStart(start),
        &catalog,
        &mut chunks,
    );
    let mut moved = player(41, 2);
    moved.location = PublicEntityLocation::Mobile {
        position: [16.5, 2.0, 0.5],
    };
    let remove = WorldCommitPart {
        commit_id: 3,
        part_index: 0,
        part_count: 2,
        key: key(0),
        epoch: 1,
        block_from: 0,
        block_to: 0,
        entity_from: 1,
        entity_to: 2,
        blocks: vec![],
        entities: vec![PublicEntityChange::Remove {
            id: 41,
            revision: 2,
        }],
    };
    let upsert = WorldCommitPart {
        key: key(1),
        part_index: 1,
        entities: vec![PublicEntityChange::Upsert(moved)],
        ..remove.clone()
    };
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldCommitPart(remove),
            &catalog,
            &mut chunks
        ),
        Assembly::Waiting
    ));
    assert_eq!(
        replicas.visual_avatars(glam::Vec3::ZERO, None)[0]
            .position
            .x,
        0.5
    );
    assert!(matches!(
        accept(&mut replicas, ServerMessage::WorldCommitPart(upsert), &catalog, &mut chunks),
        Assembly::Installed(keys) if keys == vec![key(0), key(1)]
    ));
    assert_eq!(
        replicas.visual_avatars(glam::Vec3::ZERO, None)[0]
            .position
            .x,
        16.5
    );
    assert!(replicas.entities_in(key(0)).unwrap().is_empty());
}
