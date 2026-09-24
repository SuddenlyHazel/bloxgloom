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
    process_durable_actions(context.state, context.tick, context.now)
}

pub(in crate::server) fn player_movement(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    context.movement_load = movement::advance_players(context.state, context.tick)?;
    Ok(())
}

pub(in crate::server) fn drop_simulation(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    advance_drops(context.state)
}

pub(in crate::server) fn fire_source(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    crate::server::durable::fire::run_source(context.state, context.tick)
}

pub(in crate::server) fn interaction_commit(
    context: &mut CoordinatorContext<'_>,
) -> io::Result<()> {
    commit_block_effects(context.state, context.tick)?;
    queue_interaction_actions(context.state);
    if context.state.moving_drops_dirty
        && (context.state.drops_landed_dirty
            || context.now.duration_since(context.state.last_drop_save) >= Duration::from_secs(1))
    {
        remember_drops_checkpoint(context.state)?;
        // Keep moving_drops_dirty until the matching checkpoint receipt, but
        // do not replace a captured landing snapshot on every tick.
        context.state.drops_landed_dirty = false;
        context.state.last_drop_save = context.now;
    }
    Ok(())
}

pub(in crate::server) fn fire_delivery(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    crate::server::durable::fire::run_delivery(context.state, context.tick)
}

pub(in crate::server) fn publish(context: &mut CoordinatorContext<'_>) -> io::Result<()> {
    publish_committed(context.state);
    streaming::publish_streams(context.state)
}
