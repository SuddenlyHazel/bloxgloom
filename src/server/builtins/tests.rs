use super::*;

#[test]
fn declarations_match_the_current_execution_shape() {
    let plan = builtin_phase_plan().unwrap();
    let durable = plan.systems(Phase::DurableActions);
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].partition(), OwnerPartition::Global);
    assert_eq!(durable[0].max_jobs_per_tick(), 1);

    let simulation = plan.systems(Phase::Simulation);
    let movement = simulation
        .iter()
        .find(|system| system.id().as_str() == "builtin:player_movement")
        .unwrap();
    assert_eq!(movement.partition(), OwnerPartition::Entity);
    assert_eq!(movement.max_jobs_per_tick(), MAX_CLIENTS);

    let fire = simulation
        .iter()
        .find(|system| system.id().as_str() == "bloxgloom:fire_propagate")
        .unwrap();
    assert_eq!(fire.partition(), OwnerPartition::Chunk);
    assert!(fire.has_executable_handler());
    assert_eq!(fire.max_effects_per_job(), 192);

    let interactions = plan.systems(Phase::InteractionCommit);
    let legacy = interactions
        .iter()
        .find(|system| system.id().as_str() == "builtin:interaction_commit")
        .unwrap();
    assert_eq!(legacy.partition(), OwnerPartition::Global);
    assert_eq!(legacy.max_jobs_per_tick(), 1);
    let delivery = interactions
        .iter()
        .find(|system| system.id().as_str() == "bloxgloom:fire_deliver")
        .unwrap();
    assert_eq!(delivery.partition(), OwnerPartition::Chunk);
    assert!(delivery.has_executable_handler());
}
