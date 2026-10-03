//! Adapter from public creature contracts to the existing worker/WAL boundary.
use super::*;
use crate::{content::Catalog, server::voxel_view::VoxelView};
use bloxgloom_host_api::entity as api;
use std::sync::Arc;
mod world;
use world::World;

pub(in crate::server) struct Adapter(pub Arc<api::MobileEntity>);
fn error(error: api::Error) -> EntityError {
    match error {
        api::Error::OutOfRange => EntityError::ViewOutOfRange,
        api::Error::Exhausted => EntityError::RevisionExhausted,
        api::Error::InvalidState => EntityError::InvalidPayload,
    }
}
impl EntityPayloadCodec for Adapter {
    fn validate_location(&self, location: &EntityLocation) -> Result<(), EntityError> {
        match location {
            EntityLocation::Mobile { position } if super::locomotion::valid_position(*position) => {
                Ok(())
            }
            _ => Err(EntityError::InvalidLocation),
        }
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > self.0.max_state_bytes {
            return Err(EntityCodecError::InvalidData);
        }
        let state = self
            .0
            .behavior
            .decode(bytes)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if self.encode(&state)? != bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(state)
    }
    fn encode(&self, state: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bytes = self
            .0
            .behavior
            .encode(state)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if bytes.len() > self.0.max_state_bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(bytes)
    }
    fn public_view(&self, state: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(state)?;
        let bytes = self
            .0
            .behavior
            .public(state)
            .map_err(|_| EntityCodecError::InvalidData)?;
        let pose = self
            .0
            .behavior
            .pose(&bytes)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if bytes.len() > self.0.max_public_bytes || !pose.yaw.is_finite() {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(bytes)
    }
}
impl EntityTickPolicy for Adapter {
    fn read_radius_chunks(&self) -> u8 {
        self.0.read_radius
    }
    fn reads_neighbours(&self) -> bool {
        self.0.reads_neighbours
    }
    fn wakes_on_terrain_change(&self) -> bool {
        self.0.wakes_on_terrain_change
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        catalog: &Catalog,
        view: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let EntityLocation::Mobile { position } = snapshot.location else {
            return Err(EntityError::InvalidLocation);
        };
        let world = World::new(view, self.0.body, position);
        let nearby = neighbours
            .iter()
            .map(|e| api::Neighbour {
                id: e.id.get(),
                key: catalog
                    .entity_type(e.entity_type)
                    .map_or("", |d| d.key.as_ref()),
                position: match e.location {
                    EntityLocation::Mobile { position } => position,
                    EntityLocation::Anchored { anchor, .. } => {
                        [anchor.x as f32, anchor.y as f32, anchor.z as f32]
                    }
                },
                public: &e.payload,
            })
            .collect::<Vec<_>>();
        let plan = self
            .0
            .behavior
            .tick(&api::Context {
                environment: view.environment(),
                tags: Some(catalog),
                id: snapshot.id.get(),
                tick,
                next_tick: snapshot.next_tick,
                position,
                state: &snapshot.private_payload,
                world: &world,
                neighbours: &nearby,
            })
            .map_err(error)?;
        if world.failed.get() {
            return Err(EntityError::ViewOutOfRange);
        }
        if let Some(state) = &plan.state {
            self.public_view(state)
                .map_err(|_| EntityError::InvalidPayload)?;
        }
        if let Some(next) = plan.position {
            // Movement is accepted only if returned by the host's locomotion
            // service for this snapshot, not an arbitrary extension teleport.
            if !world.movements.borrow().contains(&next) {
                return Err(EntityError::InvalidLocation);
            }
        }
        if plan.lifecycle.spawns.len() > 4 {
            return Err(EntityError::TooManyEntities);
        }
        if plan.next_tick.is_some_and(|next| next <= tick) {
            return Err(EntityError::InvalidPayload);
        }
        Ok(EntityTickPlan {
            lifecycle: plan.lifecycle,
            payload: plan.state,
            next_tick: plan.next_tick,
            position: plan.position,
            anchor_update: None,
            block_states: vec![],
            wakes: vec![],
            transfer: None,
        })
    }
}
impl EntityInteractionPolicy for Adapter {
    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &crate::inventory::Inventory,
        _catalog: &Catalog,
        view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        if request != self.0.interaction || request.is_empty() {
            return Err(EntityError::InvalidPayload);
        }
        let payload = match view.planning_tick() {
            Some(tick) => self.0.behavior.interact_at_tick(
                &snapshot.private_payload,
                request,
                snapshot.id.get(),
                snapshot.revision,
                tick,
            ),
            None => self.0.behavior.interact_at(
                &snapshot.private_payload,
                request,
                snapshot.id.get(),
                snapshot.revision,
            ),
        }
        .map_err(error)?;
        self.encode(&payload)
            .map_err(|_| EntityError::InvalidPayload)?;
        let mut inventory = inventory.clone();
        inventory.revision = inventory
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        Ok(EntityInteractionPlan {
            payload,
            inventory,
            block_states: vec![],
            wakes: vec![],
        })
    }
}
#[cfg(test)]
mod tests;
pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Catalog,
    id: crate::content::EntityTypeId,
) -> Result<(), EntityError> {
    let definition = catalog
        .mobile_entity(id)
        .cloned()
        .ok_or(EntityError::InvalidType)?;
    let adapter = Arc::new(Adapter(definition.clone()));
    builder.register(EntityTypeRegistration {
        id,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(definition.interval),
        max_payload_bytes: definition.max_state_bytes,
        codec: adapter.clone(),
    })?;
    builder.register_tick_planner(id, adapter.clone())?;
    if !definition.interaction.is_empty() {
        builder.register_interaction_policy(id, adapter)?;
    }
    Ok(())
}
pub(in crate::server) fn spawn_clear(
    view: &VoxelView,
    body: api::Body,
    position: [f32; 3],
) -> Result<bool, EntityError> {
    let body = super::locomotion::Body {
        half_width: body.half_width,
        height: body.height,
        speed: body.speed,
    };
    Ok(body.clear(view, position)? && body.grounded(view, position)?)
}
