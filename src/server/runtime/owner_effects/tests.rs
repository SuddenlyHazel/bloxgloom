use super::*;
use crate::server::parallel::{BatchId, JobKey, OwnerSnapshot, PatchUsage};
use crate::server::simulation::{Phase, TickId};

fn test_job(owner: OwnerKey) -> OwnerJob {
    let system = SystemId::new("test:effect_probe").unwrap();
    OwnerJob::new(
        system,
        JobKey::new(
            BatchId::new(TickId::new(3), Phase::Simulation, 0),
            owner,
            0,
            7,
        ),
        vec![OwnerSnapshot::new(owner, 7, Arc::new(()))],
    )
    .unwrap()
}

fn test_owner() -> OwnerKey {
    OwnerKey::Entity(11)
}

#[test]
fn plain_patches_carry_no_emissions_and_keep_their_replacement() {
    let job = test_job(test_owner());
    let patch = OwnerPatch::new(
        &job,
        OwnerData::new(9u64),
        PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: 8,
        },
    );
    assert_eq!(OwnerEffectPatch::emitted_count(&patch), 0);
    assert!(
        OwnerEffectPatch::replacement(&patch)
            .unwrap()
            .get::<u64>()
            .is_some_and(|value| *value == 9)
    );
}

#[test]
fn effect_patches_count_emissions_and_expose_their_replacement() {
    let kind = EffectKindId::new("test:probe").unwrap();
    let job = test_job(test_owner());
    let patch = OwnerPatch::new(
        &job,
        OwnerEffectPatch::new(
            OwnerData::new(4u64),
            vec![
                EmittedOwnerEffect::new(kind.clone(), 1u32),
                EmittedOwnerEffect::new(kind.clone(), 2u32),
            ],
        ),
        PatchUsage {
            writes: 1,
            effects: 2,
            estimated_bytes: 8,
        },
    );
    assert_eq!(OwnerEffectPatch::emitted_count(&patch), 2);
    assert_eq!(
        OwnerEffectPatch::replacement(&patch).unwrap().get::<u64>(),
        Some(&4)
    );
    assert_eq!(
        patch.payload::<OwnerEffectPatch>().unwrap().effects.len(),
        2
    );
    assert_eq!(
        patch.payload::<OwnerEffectPatch>().unwrap().effects[0].kind(),
        &kind
    );
}

#[test]
fn foreign_payloads_have_no_replacement() {
    let job = test_job(test_owner());
    let patch = OwnerPatch::new(
        &job,
        13u64,
        PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: 8,
        },
    );
    assert_eq!(OwnerEffectPatch::emitted_count(&patch), 0);
    assert!(OwnerEffectPatch::replacement(&patch).is_none());
}
