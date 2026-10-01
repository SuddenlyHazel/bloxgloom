//! Trusted adapters for transitional coordinator-owned systems.
//!
//! These functions are registered at startup, not selected by a runtime ID
//! switch. Fire's adapters only admit owner jobs; its gameplay computation
//! remains in registered immutable worker handlers.

use super::*;

pub(in crate::server) fn input_authorization(
    context: &mut CoordinatorContext<'_>,
) -> io::Result<()> {
    for input in context.rejected.take().unwrap_or_default() {
        reject_simulation_input(context.state, input);
    }
    for input in context.ready.take().unwrap_or_default() {
        apply_simulation_input(context.state, input, context.tick);
    }
    process_pending_joins(context.state, context.tick);
    Ok(())
}

pub(in crate::server) fn durable_actions(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    crate::server::entities::motion::colliders::sample(context.state, context.tick.get());
    process_durable_actions(context.state, context.tick, context.now)
}

pub(in crate::server) fn player_movement(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    context.movement_load = movement::advance_players(context.state, context.tick)?;
    Ok(())
}

pub(in crate::server) fn fire_source(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    crate::server::durable::fire::run_source(context.state, context.tick)
}

pub(in crate::server) fn interaction_commit(
    context: &mut CoordinatorContext<'_>,
) -> io::Result<()> {
    // Drop motion is scheduled entity work now: due drops queue as
    // `EntityTick` requests and stage through the WAL, so this barrier only
    // routes block-change wakes and queues interaction work.
    commit_block_effects(context.state, context.tick)?;
    queue_interaction_actions(context.state, context.tick);
    Ok(())
}

pub(in crate::server) fn fire_delivery(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    crate::server::durable::fire::run_delivery(context.state, context.tick)
}

pub(in crate::server) fn publish(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    publish_committed(context.state)?;
    streaming::publish_streams(context.state)
}
