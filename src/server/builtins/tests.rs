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

    let interactions = plan.systems(Phase::InteractionCommit);
    assert_eq!(interactions.len(), 1);
    assert_eq!(interactions[0].partition(), OwnerPartition::Global);
    assert_eq!(interactions[0].max_jobs_per_tick(), 1);
}
