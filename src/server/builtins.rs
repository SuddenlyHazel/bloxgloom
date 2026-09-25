//! Built-in system declarations and trusted runtime callbacks.

use super::MAX_CLIENTS;
use super::fire::{FireDeliveryHandler, FireHandler};
use super::registry::{
    OwnerPartition, PhasePlan, ResourceId, SystemDescriptor, SystemId, SystemRegistry,
};
use super::runtime::adapters;
use super::simulation::Phase;
use std::io;

#[cfg(test)]
#[path = "builtins/tests.rs"]
mod tests;

pub(super) fn builtin_phase_plan() -> io::Result<PhasePlan> {
    let mut registry = SystemRegistry::new();
    register_builtin_systems(&mut registry)?;
    let plan = registry
        .freeze()
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    for phase in Phase::ALL {
        for system in plan.systems(phase) {
            if system.driver().is_none() {
                return Err(io::Error::other(format!(
                    "built-in system lacks a trusted runtime driver: {}",
                    system.id().as_str()
                )));
            }
        }
    }
    Ok(plan)
}

pub(super) fn register_builtin_systems(registry: &mut SystemRegistry) -> io::Result<()> {
    let id = |name: &str| {
        SystemId::new(format!("builtin:{name}"))
            .map_err(|error| io::Error::other(format!("system ID: {error:?}")))
    };
    let resource = |name: &str| {
        ResourceId::new(format!("bloxgloom:{name}"))
            .map_err(|error| io::Error::other(format!("resource ID: {error:?}")))
    };
    let auth = id("input_authorization")?;
    let durable = id("durable_actions")?;
    let movement = id("player_movement")?;
    let interactions = id("interaction_commit")?;
    let publish = id("publish")?;
    let fire = SystemId::new("bloxgloom:fire_propagate")
        .map_err(|error| io::Error::other(format!("system ID: {error:?}")))?;
    let fire_delivery = SystemId::new("bloxgloom:fire_deliver")
        .map_err(|error| io::Error::other(format!("system ID: {error:?}")))?;
    registry
        .register_coordinator_adapter(
            SystemDescriptor::new(
                auth.clone(),
                Phase::InputAuthorization,
                OwnerPartition::Global,
                1_024,
                0,
            )
            .read(resource("input_queue")?)
            .write(resource("authorized_actions")?),
            adapters::input_authorization,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_coordinator_adapter(
            SystemDescriptor::new(
                durable.clone(),
                Phase::DurableActions,
                // Transactions may span profiles, chunks, and drops. The
                // coordinator executes one global commit batch today; the
                // declaration must not promise independent profile jobs.
                OwnerPartition::Global,
                1,
                1_024,
            )
            .read(resource("authorized_actions")?)
            .read(resource("world")?)
            .read(resource("inventory")?)
            .read(resource("entities")?)
            .write(resource("journal")?)
            .write(resource("committed_state")?)
            .after(auth.clone()),
            adapters::durable_actions,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_coordinator_adapter(
            SystemDescriptor::new(
                movement.clone(),
                Phase::Simulation,
                OwnerPartition::Entity,
                MAX_CLIENTS,
                0,
            )
            .read(resource("world")?)
            .read(resource("movement_inputs")?)
            .write(resource("player_positions")?)
            .after(durable.clone()),
            adapters::player_movement,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_handler_with_driver(
            SystemDescriptor::new(
                fire.clone(),
                Phase::Simulation,
                OwnerPartition::Chunk,
                32,
                6_144,
            )
            .effects_per_job(192)
            .read(resource("world")?)
            .read(resource("fire_frontier")?)
            .write(resource("fire_intents")?)
            .after(movement.clone()),
            FireHandler,
            adapters::fire_source,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_coordinator_adapter(
            SystemDescriptor::new(
                interactions.clone(),
                Phase::InteractionCommit,
                // Effects route by owner, but the current commit loop still
                // runs as one coordinator-owned batch.
                OwnerPartition::Global,
                1,
                4_096,
            )
            .read(resource("committed_state")?)
            .read(resource("entities")?)
            .write(resource("deferred_actions")?)
            .after(movement.clone()),
            adapters::interaction_commit,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_handler_with_driver(
            SystemDescriptor::new(
                fire_delivery,
                Phase::InteractionCommit,
                OwnerPartition::Chunk,
                32,
                0,
            )
            .read(resource("world")?)
            .read(resource("fire_pending")?)
            .read(resource("fire_frontier")?)
            .write(resource("fire_intents")?)
            .after(interactions.clone()),
            FireDeliveryHandler,
            adapters::fire_delivery,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register_coordinator_adapter(
            SystemDescriptor::new(
                publish.clone(),
                Phase::Publish,
                OwnerPartition::Global,
                MAX_CLIENTS,
                4_096,
            )
            .read(resource("committed_state")?)
            .write(resource("outbound")?)
            .after(interactions),
            adapters::publish,
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    Ok(())
}
