//! Pure-physics tests for the registered drop tick planner.
use super::*;
use crate::content::{Catalog, EntityTypeId};
use crate::inventory::Stack;
use crate::items::ItemId;
use crate::server::entities::{EntityOwner, EntityView};
use crate::world::{MAX_GENERATED_HEIGHT, World};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

fn test_world() -> (World, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-drop-tick-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let world = World::new(7, root.clone()).unwrap();
    (world, root)
}

/// Captures the 27-chunk neighbourhood around `center` after forcing every
/// chunk resident, mirroring what `capture_view_for_plan` assembles.
fn neighbourhood_view(world: &mut World, center: [f32; 3]) -> VoxelView {
    use crate::world::world_to_chunk;
    let (base, _) = world_to_chunk(
        center[0].floor() as i32,
        center[1].floor() as i32,
        center[2].floor() as i32,
    );
    let mut chunks = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let key = ChunkKey {
                    x: base.x + dx,
                    y: base.y + dy,
                    z: base.z + dz,
                };
                world
                    .get_block(key.x * 16 + 8, key.y * 16 + 8, key.z * 16 + 8)
                    .unwrap();
                chunks.push(world.cached_arc_chunk(key).unwrap());
            }
        }
    }
    VoxelView::from_resident_chunks_in(chunks, world.catalog_arc()).unwrap()
}

fn drop_snapshot(id: u64, position: [f32; 3], speed: f32) -> EntitySnapshot {
    let catalog = Catalog::builtins();
    let payload = DropEntityPayload::new(Stack::new(ItemId::new(1), 3), 1_000, Duration::ZERO)
        .with_vertical_speed(speed)
        .into_entity_payload();
    let location = EntityLocation::Mobile { position };
    let owner = location.owner().unwrap();
    assert_eq!(catalog.item(ItemId::new(1)).is_some(), true);
    EntitySnapshot {
        id: crate::server::entities::EntityId::new(id).unwrap(),
        entity_type: EntityTypeId(1),
        revision: 1,
        motion_revision: 1,
        owner,
        location,
        private_payload: payload,
        next_tick: Some(9),
    }
}

fn neighbours() -> EntityView {
    EntityView::assemble(
        Vec::new(),
        crate::server::entities::EntityId::new(1).unwrap(),
    )
}

#[test]
fn falling_drop_integrates_exactly_one_fixed_step() {
    let (mut world, root) = test_world();
    let position = [0.5, MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    let view = neighbourhood_view(&mut world, position);
    let catalog = world.catalog_arc();
    let snapshot = drop_snapshot(7, position, 0.0);
    let plan = DropTickPlanner
        .plan(&snapshot, 9, &catalog, &view, &neighbours())
        .unwrap();
    let dt = FIXED_STEP.as_secs_f32().min(0.1);
    let speed = (0.0 - GRAVITY * dt).max(-TERMINAL_SPEED);
    let expected_y = position[1] - DROP_RADIUS + speed * dt + DROP_RADIUS;
    assert_eq!(plan.position, Some([position[0], expected_y, position[2]]));
    let payload = plan.payload.unwrap().downcast_ref::<DropEntityPayload>().unwrap().clone();
    assert_eq!(payload.vertical_speed.to_bits(), speed.to_bits());
    assert_eq!(plan.next_tick, Some(10));
    assert!(plan.transfer.is_none());
    assert!(plan.anchor_update.is_none());
    assert!(plan.block_states.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dropped_column_lands_at_rest_and_suspends() {
    let (mut world, root) = test_world();
    let catalog = world.catalog_arc();
    // Find generated ground under x=z=0 by scanning down to solid rock.
    let mut surface_top = None;
    for y in (-64..600).rev() {
        let block = world.get_block(0, y, 0).unwrap();
        if catalog.block_flags(block) & crate::content::SOLID != 0 {
            surface_top = Some(y as f32 + 1.0);
            break;
        }
    }
    let top = surface_top.expect("generated terrain has solid ground");
    let mut position = [0.5, top + 3.0, 0.5];
    let mut speed = 0.0f32;
    let mut steps = 0;
    let rest = loop {
        let view = neighbourhood_view(&mut world, position);
        let snapshot = drop_snapshot(7, position, speed);
        let plan = DropTickPlanner
            .plan(&snapshot, 9 + steps, &catalog, &view, &neighbours())
            .unwrap();
        steps += 1;
        assert!(steps < 5_000, "a 3-block fall must settle");
        if plan.next_tick.is_none() {
            if let Some(next) = plan.position {
                position = next;
            }
            break position;
        }
        position = plan.position.expect("airborne ticks always move");
        speed = plan
            .payload
            .as_ref()
            .and_then(|payload| payload.downcast_ref::<DropEntityPayload>())
            .map(|payload| payload.vertical_speed)
            .unwrap_or(speed);
    };
    assert_eq!(rest[0], 0.5);
    assert_eq!(rest[2], 0.5);
    assert_eq!(rest[1].to_bits(), (top + DROP_RADIUS).to_bits());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn resting_drop_plans_no_work_and_reaffirms() {
    let (mut world, root) = test_world();
    let catalog = world.catalog_arc();
    let mut surface_top = None;
    for y in (-64..600).rev() {
        let block = world.get_block(0, y, 0).unwrap();
        if catalog.block_flags(block) & crate::content::SOLID != 0 {
            surface_top = Some(y as f32 + 1.0);
            break;
        }
    }
    let top = surface_top.expect("generated terrain has solid ground");
    let position = [0.5, top + DROP_RADIUS, 0.5];
    let view = neighbourhood_view(&mut world, position);
    let mut snapshot = drop_snapshot(7, position, 0.0);
    snapshot.next_tick = None;
    let plan = DropTickPlanner
        .plan(&snapshot, 41, &catalog, &view, &neighbours())
        .unwrap();
    // A woken settled drop must reaffirm: no payload, no motion, no schedule.
    assert!(plan.payload.is_none());
    assert!(plan.position.is_none());
    assert!(plan.next_tick.is_none());
    assert!(plan.transfer.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_terrain_defers_fail_closed_without_guessing() {
    let (mut world, root) = test_world();
    let position = [0.5, MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    world.get_block(0, 512, 0).unwrap();
    let chunk = world
        .cached_arc_chunk(crate::world::world_to_chunk(0, 512, 0).0)
        .unwrap();
    let partial = VoxelView::from_resident_chunks_in(vec![chunk], world.catalog_arc()).unwrap();
    let catalog = world.catalog_arc();
    let snapshot = drop_snapshot(7, position, 0.0);
    assert!(matches!(
        DropTickPlanner.plan(&snapshot, 9, &catalog, &partial, &neighbours()),
        Err(EntityError::ViewOutOfRange)
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn planner_rejects_anchored_locations_and_foreign_payloads() {
    let (mut world, root) = test_world();
    let position = [0.5, MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    let view = neighbourhood_view(&mut world, position);
    let catalog = world.catalog_arc();
    let owner = EntityOwner::Mobile(crate::world::world_to_chunk(0, 512, 0).0);
    let anchored = EntitySnapshot {
        id: crate::server::entities::EntityId::new(7).unwrap(),
        entity_type: EntityTypeId(1),
        revision: 1,
        motion_revision: 1,
        owner,
        location: EntityLocation::Anchored {
            anchor: crate::server::entities::CellCoord::new(0, 512, 0),
            anchor_state: crate::content::BlockStateId(1),
            footprint: vec![crate::server::entities::CellCoord::new(0, 512, 0)],
        },
        private_payload: crate::server::entities::EntityPayload::new(3u8),
        next_tick: Some(9),
    };
    assert!(matches!(
        DropTickPlanner.plan(&anchored, 9, &catalog, &view, &neighbours()),
        Err(EntityError::WrongOwnership)
    ));
    let mut foreign = drop_snapshot(7, position, 0.0);
    foreign.private_payload = crate::server::entities::EntityPayload::new(3u8);
    assert!(matches!(
        DropTickPlanner.plan(&foreign, 9, &catalog, &view, &neighbours()),
        Err(EntityError::InvalidPayload)
    ));
    std::fs::remove_dir_all(root).unwrap();
}
