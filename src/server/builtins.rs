//! Built-in system declarations and their typed runtime dispatch identities.

use super::MAX_CLIENTS;
use super::registry::{
    OwnerPartition, PhasePlan, ResourceId, SystemDescriptor, SystemId, SystemRegistry,
};
use super::simulation::Phase;
use std::io;

#[cfg(test)]
#[path = "builtins/tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(super) enum BuiltinHandler {
    InputAuthorization,
    DurableActions,
    PlayerMovement,
    DropSimulation,
    InteractionCommit,
    Publish,
}

impl BuiltinHandler {
    pub(super) fn from_id(id: &str) -> io::Result<Self> {
        match id {
            "builtin:input_authorization" => Ok(Self::InputAuthorization),
            "builtin:durable_actions" => Ok(Self::DurableActions),
            "builtin:player_movement" => Ok(Self::PlayerMovement),
            "builtin:drop_simulation" => Ok(Self::DropSimulation),
            "builtin:interaction_commit" => Ok(Self::InteractionCommit),
            "builtin:publish" => Ok(Self::Publish),
            unknown => Err(io::Error::other(format!(
                "registered system has no server dispatcher: {unknown}"
            ))),
        }
    }
}

pub(super) fn builtin_phase_plan() -> io::Result<PhasePlan> {
    let mut registry = SystemRegistry::new();
    register_builtin_systems(&mut registry)?;
    registry
        .freeze_legacy([
            SystemId::new("builtin:input_authorization").unwrap(),
            SystemId::new("builtin:durable_actions").unwrap(),
            SystemId::new("builtin:player_movement").unwrap(),
            SystemId::new("builtin:drop_simulation").unwrap(),
            SystemId::new("builtin:interaction_commit").unwrap(),
            SystemId::new("builtin:publish").unwrap(),
        ])
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))
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
    let simulation = id("drop_simulation")?;
    let interactions = id("interaction_commit")?;
    let publish = id("publish")?;
    registry
        .register(
            SystemDescriptor::new(
                auth.clone(),
                Phase::InputAuthorization,
                OwnerPartition::Global,
                1_024,
                0,
            )
            .read(resource("input_queue")?)
            .write(resource("authorized_actions")?),
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register(
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
            .read(resource("drops")?)
            .write(resource("journal")?)
            .write(resource("committed_state")?)
            .after(auth.clone()),
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register(
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
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register(
            SystemDescriptor::new(
                simulation.clone(),
                Phase::Simulation,
                // Drop motion is one coordinator-owned batch today. Register
                // the real execution shape, not a speculative per-drop job.
                OwnerPartition::Global,
                1,
                0,
            )
            .read(resource("world")?)
            .read(resource("drops")?)
            .write(resource("drops")?)
            .after(movement.clone()),
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register(
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
            .read(resource("drops")?)
            .write(resource("drops")?)
            .write(resource("deferred_actions")?)
            .after(simulation.clone()),
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    registry
        .register(
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
        )
        .map_err(|error| io::Error::other(format!("system registry: {error:?}")))?;
    Ok(())
}
