use super::*;
use crate::server::parallel::{BatchId, JobKey, OwnerSnapshot, PatchUsage};
use crate::server::simulation::TickId;
use crate::world::ChunkKey;

#[derive(Clone)]
struct TestPayload {
    destinations: Vec<OwnerKey>,
    value: u32,
}

fn chunk(x: i32) -> OwnerKey {
    OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 })
}

fn registry() -> (EffectKindRegistryFrozen, EffectKindId) {
    let id = EffectKindId::new("test:ignite").unwrap();
    let mut registry = EffectKindRegistry::new();
    registry
        .register(
            id.clone(),
            1,
            64,
            8,
            |payload: &TestPayload| 4 + payload.destinations.len() * 16,
            |payload: &TestPayload| {
                if payload.value == 0 {
                    Err("zero value".to_owned())
                } else {
                    Ok(())
                }
            },
            |payload: &TestPayload| Ok(payload.destinations.clone()),
            |_job, payloads: &[&TestPayload]| {
                let sum = payloads.iter().map(|payload| payload.value).sum::<u32>();
                Ok(EffectConsumerOutput::new(
                    sum,
                    PatchUsage {
                        writes: payloads.len(),
                        effects: 0,
                        estimated_bytes: 4,
                    },
                ))
            },
        )
        .unwrap();
    (registry.freeze(), id)
}

fn emitted(
    registry: &EffectKindRegistryFrozen,
    id: &EffectKindId,
    source: OwnerKey,
    payload: TestPayload,
) -> RegisteredEffectIntent {
    let mut buffer = RegisteredEffectBuffer::new(
        TickId::new(9),
        Phase::Simulation,
        SystemId::new("test:fire").unwrap(),
        source,
        4,
        registry,
    )
    .unwrap();
    buffer.emit(id, payload).unwrap();
    buffer.finish().unwrap().pop().unwrap()
}

fn keys(batch: &RoutedEffectBatch) -> Vec<(OwnerKey, RegisteredEffectOrderKey, usize)> {
    batch
        .owners()
        .iter()
        .flat_map(|owner| {
            owner.effects.iter().map(|effect| {
                (
                    owner.owner,
                    effect.intent().key().clone(),
                    effect.intent().payload_bytes(),
                )
            })
        })
        .collect()
}

#[test]
fn typed_registry_routes_expanded_deliveries_in_full_stable_order() {
    let (registry, id) = registry();
    assert_eq!(registry.kind_count(), 1);
    assert_eq!(registry.schema_version(&id), Some(1));
    let early = emitted(
        &registry,
        &id,
        chunk(4),
        TestPayload {
            destinations: vec![chunk(-1), chunk(4)],
            value: 13,
        },
    );
    let late = emitted(
        &registry,
        &id,
        chunk(5),
        TestPayload {
            destinations: vec![chunk(4)],
            value: 17,
        },
    );

    let first = route_registered_effects(
        [late.clone(), early.clone()],
        RegisteredEffectLimits::default(),
    )
    .unwrap();
    let second = route_registered_effects(
        [early.clone(), late.clone()],
        RegisteredEffectLimits::default(),
    )
    .unwrap();
    assert_eq!(keys(&first), keys(&second));
    assert_eq!(first.delivery_count(), 3);
    assert_eq!(first.owners().len(), 2);
}

#[test]
fn registered_consumer_builds_a_typed_scratch_patch_for_its_destination() {
    let (registry, id) = registry();
    let destination = chunk(-1);
    let intent = emitted(
        &registry,
        &id,
        chunk(2),
        TestPayload {
            destinations: vec![destination],
            value: 42,
        },
    );
    let batch = route_registered_effects([intent], RegisteredEffectLimits::default()).unwrap();
    let job = OwnerJob::new(
        SystemId::new("test:effect_consumer").unwrap(),
        JobKey::new(
            BatchId::new(TickId::new(9), Phase::InteractionCommit, 0),
            destination,
            0,
            3,
        ),
        vec![OwnerSnapshot::new(destination, 3, std::sync::Arc::new(()))],
    )
    .unwrap();

    let patch = batch.owners()[0].prepare_consumer(&job).unwrap();
    assert_eq!(patch.owner(), destination);
    assert_eq!(
        patch
            .payload::<EffectConsumerBatch>()
            .unwrap()
            .output::<u32>(&id),
        Some(&42)
    );
    assert_eq!(patch.usage().writes, 1);
}

#[test]
fn routed_consumer_runs_once_with_stable_owner_kind_batch() {
    let (registry, id) = registry();
    let destination = chunk(4);
    let system = SystemId::new("test:fire").unwrap();
    let mut output = RegisteredEffectBuffer::new(
        TickId::new(9),
        Phase::Simulation,
        system,
        chunk(2),
        4,
        &registry,
    )
    .unwrap();
    output
        .emit(
            &id,
            TestPayload {
                destinations: vec![destination],
                value: 11,
            },
        )
        .unwrap();
    output
        .emit(
            &id,
            TestPayload {
                destinations: vec![destination],
                value: 31,
            },
        )
        .unwrap();
    let batch =
        route_registered_effects(output.finish().unwrap(), RegisteredEffectLimits::default())
            .unwrap();
    assert_eq!(batch.delivery_count(), 2);
    let job = OwnerJob::new(
        SystemId::new("test:effect_consumer").unwrap(),
        JobKey::new(
            BatchId::new(TickId::new(9), Phase::InteractionCommit, 0),
            destination,
            0,
            3,
        ),
        vec![OwnerSnapshot::new(destination, 3, std::sync::Arc::new(()))],
    )
    .unwrap();
    let patch = batch.owners()[0].prepare_consumer(&job).unwrap();
    assert_eq!(patch.usage().writes, 2);
    assert_eq!(
        patch
            .payload::<EffectConsumerBatch>()
            .unwrap()
            .output::<u32>(&id),
        Some(&42)
    );
}

#[test]
fn oversized_expanded_fanout_aborts_routing_before_any_batch_is_returned() {
    let (registry, id) = registry();
    let effect = emitted(
        &registry,
        &id,
        chunk(0),
        TestPayload {
            destinations: vec![chunk(-1), chunk(0)],
            value: 2,
        },
    );
    assert!(matches!(
        route_registered_effects(
            [effect],
            RegisteredEffectLimits {
                total_deliveries: 1,
                per_destination: 4,
                payload_bytes: MAX_EFFECT_BATCH_PAYLOAD_BYTES,
            },
        ),
        Err(RouteRegisteredError::TotalOverflow { limit: 1 })
    ));
}

#[test]
fn invalid_payload_and_duplicate_destinations_are_rejected() {
    let (registry, id) = registry();
    let mut output = RegisteredEffectBuffer::new(
        TickId::new(1),
        Phase::Simulation,
        SystemId::new("test:fire").unwrap(),
        chunk(0),
        4,
        &registry,
    )
    .unwrap();
    assert!(matches!(
        output.emit(
            &id,
            TestPayload {
                destinations: vec![chunk(0)],
                value: 0,
            }
        ),
        Err(RegisteredEffectError::InvalidPayload { .. })
    ));
    output
        .emit(
            &id,
            TestPayload {
                destinations: vec![chunk(0), chunk(0)],
                value: 1,
            },
        )
        .unwrap();
    assert!(matches!(
        route_registered_effects(output.finish().unwrap(), RegisteredEffectLimits::default()),
        Err(RouteRegisteredError::InvalidEffect(
            RegisteredEffectError::DuplicateDestination { .. }
        ))
    ));
}
