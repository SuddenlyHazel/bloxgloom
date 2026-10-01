use super::registry::EntityAdapter;
use super::*;
#[path = "kiln_tests.rs"]
mod kiln_tests;
#[path = "mossbun_tests.rs"]
mod mossbun_tests;
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
        payload: vec![1, 2, 3, 0],
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
    replicas.accept(
        message,
        catalog,
        chunks,
        &EntityClientRegistry::builtins(catalog),
    )
}

fn kiln_hit() -> crate::raycast::Hit {
    use crate::raycast::{Face, Hit};
    Hit {
        block: [5, 12, -3],
        adjacent: [5, 12, -2],
        block_id: crate::content::KILN_DEFAULT_STATE,
        distance: 2.0,
        face: Face::PosZ,
    }
}

#[test]
fn kiln_registry_path_emits_byte_identical_requests() {
    let catalog = Catalog::builtins();
    let hit = kiln_hit();
    let registry = EntityClientRegistry::builtins(&Catalog::builtins());
    assert!(registry.handles(hit, &catalog));
    // Every kiln command pinned: legacy spelling and registry path must agree
    // byte-for-byte, preserving the replication contract.
    let cases = [
        (
            kiln::KilnCommand::InsertInput,
            kiln::INSERT_INPUT,
            [1, 0, 1, 4, 1, 0],
        ),
        (
            kiln::KilnCommand::TakeOutput,
            kiln::TAKE_OUTPUT,
            [1, 1, 2, 4, 1, 0],
        ),
        (
            kiln::KilnCommand::InsertFuel,
            kiln::INSERT_FUEL,
            [1, 0, 0, 4, 1, 0],
        ),
        (
            kiln::KilnCommand::TakeFuel,
            kiln::TAKE_FUEL,
            [1, 1, 0, 4, 1, 0],
        ),
    ];
    for (command, verb, payload) in cases {
        let action_id = (1u128 << 64) | u128::from(payload[1]) << 32 | u128::from(payload[2]);
        let legacy = kiln::interaction(hit, action_id, 4, command);
        let via_registry = registry
            .interact(hit, &catalog, action_id, 4, verb)
            .expect("registered kiln verb must build a request");
        assert_eq!(legacy, via_registry);
        let ClientMessage::EntityInteract {
            action_id: sent_id,
            target,
            payload: sent,
        } = via_registry
        else {
            panic!("kiln verbs must use the generic interaction wire");
        };
        assert_eq!(sent_id, action_id);
        assert_eq!(target, hit.block);
        assert_eq!(sent, payload);
    }
    assert!(
        registry
            .interact(hit, &catalog, 1u128 << 64 | 1, 0, "kiln:unknown-verb")
            .is_none()
    );
    let mut ordinary = hit;
    ordinary.block_id = STONE;
    assert!(!registry.handles(ordinary, &catalog));
    assert!(
        registry
            .interact(ordinary, &catalog, 1u128 << 64 | 1, 0, kiln::TAKE_OUTPUT)
            .is_none()
    );
}

// A test-only entity type presented purely by registration: no edit to the
// assembler, the window dispatch, or any other core client module.
const PROBE_TYPE: EntityTypeId = EntityTypeId(9501);
const PROBE_VERB: &str = "probe:ping";
const PROBE_ANCHOR: [i32; 3] = [9, 9, 9];

fn probe_avatar(entity: &PublicEntity) -> Result<Option<crate::render::VisualAvatar>, ()> {
    let PublicEntityLocation::Mobile { position } = &entity.location else {
        return Err(());
    };
    if entity.payload != [0xA5, 0x5A] {
        return Err(());
    }
    Ok(Some(crate::render::VisualAvatar {
        character_pose: [0.0; 3],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
        animation: Default::default(),
        model: crate::render::AvatarModel::Player,
        pose: [0.0; 4],
        airborne: false,
        id: entity.id,
        position: glam::Vec3::from_array(*position),
        cosmetics: [0xA5, 0x5A, 0, 0],
        light_levels: [0; 4],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }))
}

fn probe_hit(hit: crate::raycast::Hit, _: &Catalog) -> bool {
    hit.block == PROBE_ANCHOR
}

fn probe_interact(
    hit: crate::raycast::Hit,
    action_id: u128,
    hotbar_slot: u8,
    verb: &str,
) -> Option<ClientMessage> {
    if verb != PROBE_VERB {
        return None;
    }
    Some(ClientMessage::EntityInteract {
        action_id,
        target: hit.block,
        payload: vec![0xFE, hotbar_slot],
    })
}

fn probe_registry() -> EntityClientRegistry {
    let mut registry = EntityClientRegistry::builtins(&Catalog::builtins());
    registry.register(EntityAdapter {
        entity_type: PROBE_TYPE,
        project_avatar: probe_avatar,
        hit_test: probe_hit,
        interact: probe_interact,
    });
    registry
}

fn probe_entity(id: u64, revision: u64) -> PublicEntity {
    PublicEntity {
        id,
        entity_type: PROBE_TYPE,
        revision,
        motion_revision: revision,
        location: PublicEntityLocation::Mobile {
            position: [7.5, 3.0, 1.5],
        },
        payload: vec![0xA5, 0x5A],
    }
}

#[test]
fn test_only_type_presents_and_interacts_by_registration_only() {
    use crate::raycast::{Face, Hit};
    let catalog = Catalog::builtins();
    let registry = probe_registry();
    let mut entities = BTreeMap::new();
    entities.insert(1, player(1, 1));
    entities.insert(2, probe_entity(2, 1));

    let avatars = registry
        .project(&entities)
        .expect("both views are well-formed");
    assert_eq!(avatars.len(), 2);
    let probe = avatars.iter().find(|avatar| avatar.id == 2).unwrap();
    assert_eq!(probe.position, glam::Vec3::new(7.5, 3.0, 1.5));
    assert_eq!(probe.cosmetics, [0xA5, 0x5A, 0, 0]);

    let hit = Hit {
        block: PROBE_ANCHOR,
        adjacent: PROBE_ANCHOR,
        block_id: STONE,
        distance: 2.0,
        face: Face::PosY,
    };
    assert!(registry.handles(hit, &catalog));
    let action_id = (1u128 << 64) | 7;
    assert_eq!(
        registry.interact(hit, &catalog, action_id, 3, PROBE_VERB),
        Some(ClientMessage::EntityInteract {
            action_id,
            target: PROBE_ANCHOR,
            payload: vec![0xFE, 3],
        })
    );
    assert!(
        registry
            .interact(hit, &catalog, action_id, 3, "probe:unknown")
            .is_none()
    );

    let mut corrupt = probe_entity(3, 1);
    corrupt.payload = vec![0x00];
    entities.insert(3, corrupt);
    assert!(registry.project(&entities).is_err());
}

#[test]
fn unknown_entity_type_degrades_safely() {
    let catalog = Catalog::builtins();
    let registry = EntityClientRegistry::builtins(&Catalog::builtins());
    // Unknown types are stored by the assembler but draw nothing and panic
    // nowhere. (The wire decoder rejects unregistered types before assembly,
    // so the projection layer only ever has to skip them.)
    let mut entities = BTreeMap::new();
    entities.insert(1, player(1, 1));
    entities.insert(
        2,
        PublicEntity {
            entity_type: EntityTypeId(7777),
            ..player(2, 1)
        },
    );
    let avatars = registry.project(&entities).unwrap();
    assert_eq!(avatars.len(), 1);
    assert_eq!(avatars[0].id, 1);

    let mut ordinary = kiln_hit();
    ordinary.block_id = STONE;
    assert!(!registry.handles(ordinary, &catalog));
    assert!(
        registry
            .interact(ordinary, &catalog, 1u128 << 64 | 1, 0, kiln::TAKE_OUTPUT)
            .is_none()
    );
}

#[test]
fn presentation_never_influences_inventory_ownership() {
    use crate::inventory::Inventory;
    let catalog = Catalog::builtins();
    let registry = EntityClientRegistry::builtins(&Catalog::builtins());
    let mut entities = BTreeMap::new();
    entities.insert(1, player(1, 1));
    entities.insert(
        2,
        PublicEntity {
            entity_type: EntityTypeId(7777),
            ..player(2, 1)
        },
    );

    // Presentation takes only shared snapshots and returns owned values, so a
    // full project + interact cycle must leave player inventory untouched.
    let inventory = Inventory::default();
    let before = inventory.clone();
    let _ = registry.project(&entities).unwrap();
    let hit = kiln_hit();
    let _ = registry.handles(hit, &catalog);
    let message = registry
        .interact(hit, &catalog, 1u128 << 64 | 1, 4, kiln::TAKE_OUTPUT)
        .unwrap();
    assert_eq!(inventory, before);
    // The only thing presentation may emit is a bounded opaque request; the
    // server resolves reach and owns every slot decision.
    let ClientMessage::EntityInteract { payload, .. } = message else {
        panic!("entity presentation must only emit the generic interaction wire");
    };
    assert_eq!(payload, [1, 1, 2, 4, 1, 0]);
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
    assert_eq!(replicas.take_installed_cells(), (vec![], false));
    assert!(matches!(
        accept(&mut replicas, ServerMessage::WorldCommitPart(first.clone()), &catalog, &mut chunks),
        Assembly::Installed(keys) if keys == vec![key(0), key(1)]
    ));
    assert_eq!(chunks[&key(0)].block([0, 0, 0]), Some(STONE));
    assert_eq!(chunks[&key(1)].block([1, 0, 0]), Some(DIRT));
    assert_eq!(
        replicas.take_installed_cells(),
        (vec![(key(0), [0, 0, 0]), (key(1), [1, 0, 0])], false)
    );
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
fn entity_only_commit_notifies_replica_presentation_without_chunk_remesh() {
    let catalog = Catalog::builtins();
    let registry = EntityClientRegistry::builtins(&catalog);
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let k = key(0);
    let (start, _) = snapshot(k, 1, 0, vec![], &catalog);
    assert!(matches!(
        replicas.accept(
            ServerMessage::WorldSnapshotStart(start),
            &catalog,
            &mut chunks,
            &registry
        ),
        Assembly::Installed(_)
    ));
    let mut creature = player(12, 1);
    creature.entity_type = crate::content::MOSSBUN_ENTITY_TYPE;
    creature.payload = vec![0, 1];
    let result = replicas.accept(
        ServerMessage::WorldCommitPart(WorldCommitPart {
            commit_id: 1,
            part_index: 0,
            part_count: 1,
            key: k,
            epoch: 1,
            block_from: 0,
            block_to: 0,
            entity_from: 0,
            entity_to: 1,
            blocks: vec![],
            entities: vec![PublicEntityChange::Upsert(creature)],
        }),
        &catalog,
        &mut chunks,
        &registry,
    );
    assert!(matches!(result, Assembly::Installed(keys) if keys.is_empty()));
    assert_eq!(replicas.presentation_entities("bloxgloom", &catalog).1, 1);
    assert_eq!(chunks[&k].version, 0);
}

#[test]
fn motion_only_upserts_install_without_resync_but_conflicts_do_not() {
    let catalog = Catalog::builtins();
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let original = player(12, 1);
    let (start, pages) = snapshot(key(0), 1, 1, vec![vec![original.clone()]], &catalog);
    accept(
        &mut replicas,
        ServerMessage::WorldSnapshotStart(start),
        &catalog,
        &mut chunks,
    );
    for page in pages {
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(page),
            &catalog,
            &mut chunks,
        );
    }
    let mut moved = original;
    moved.motion_revision = 2;
    moved.location = PublicEntityLocation::Mobile {
        position: [1.5, 2.0, 0.5],
    };
    let part = WorldCommitPart {
        commit_id: 1,
        part_index: 0,
        part_count: 1,
        key: key(0),
        epoch: 1,
        block_from: 0,
        block_to: 0,
        entity_from: 1,
        entity_to: 2,
        blocks: vec![],
        entities: vec![PublicEntityChange::Upsert(moved.clone())],
    };
    assert!(matches!(
        accept(
            &mut replicas,
            ServerMessage::WorldCommitPart(part.clone()),
            &catalog,
            &mut chunks
        ),
        Assembly::Installed(keys) if keys.is_empty()
    ));
    assert_eq!(replicas.entities_in(key(0)).unwrap()[&12], moved);
    for case in 0..3 {
        let mut invalid = moved.clone();
        match case {
            0 => invalid.motion_revision = 1,
            1 => {
                invalid.location = PublicEntityLocation::Mobile {
                    position: [2.5, 2.0, 0.5],
                }
            }
            _ => {
                invalid.motion_revision = 3;
                invalid.payload[0] ^= 1;
            }
        }
        let invalid_part = WorldCommitPart {
            commit_id: 2 + case,
            entity_from: 2,
            entity_to: 3,
            entities: vec![PublicEntityChange::Upsert(invalid)],
            ..part.clone()
        };
        assert!(matches!(
            accept(
                &mut replicas,
                ServerMessage::WorldCommitPart(invalid_part),
                &catalog,
                &mut chunks
            ),
            Assembly::Resync(_)
        ));
        assert_eq!(replicas.entities_in(key(0)).unwrap()[&12], moved);
    }
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
    let original_west = Arc::clone(&chunks[&key(0)]);
    let original_east = Arc::clone(&chunks[&key(1)]);
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
        accept(
            &mut replicas,
            ServerMessage::WorldCommitPart(upsert),
            &catalog,
            &mut chunks
        ),
        Assembly::Installed(keys) if keys.is_empty()
    ));
    assert!(Arc::ptr_eq(&original_west, &chunks[&key(0)]));
    assert!(Arc::ptr_eq(&original_east, &chunks[&key(1)]));
    assert_eq!(
        replicas.visual_avatars(glam::Vec3::ZERO, None)[0]
            .position
            .x,
        16.5
    );
    assert!(replicas.entities_in(key(0)).unwrap().is_empty());
}

#[test]
fn player_projection_preserves_recipe_and_rejects_noncanonical_flags() {
    let registry = EntityClientRegistry::builtins(&Catalog::builtins());
    let recipe = crate::appearance::CharacterRecipe {
        hair: 2,
        eyes: 6,
        mouth: 3,
        iris: Some([25, 130, 240]),
    };
    let appearance = crate::appearance::AppearanceState {
        palettes: [1, 2, 3],
        character: Some(recipe),
    };
    let mut entity = player(1, 1);
    entity.payload = appearance.encode();
    let projected = registry
        .project(&BTreeMap::from([(1, entity.clone())]))
        .unwrap();
    assert_eq!(projected[0].character_recipe, Some(recipe));
    assert_eq!(projected[0].cosmetics, [1, 2, 3, 0]);
    entity.payload[3] = 4;
    assert!(registry.project(&BTreeMap::from([(1, entity)])).is_err());
}
