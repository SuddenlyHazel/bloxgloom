//! General anchored own-state adapter on the existing worker/WAL boundary.
use super::*;
use crate::{content::Catalog, server::voxel_view::VoxelView};
use bloxgloom_host_api::anchored as api;
use std::sync::Arc;

pub(in crate::server) struct Adapter {
    pub definition: Arc<api::AnchoredBlockEntity>,
    pub catalog: Arc<Catalog>,
}
impl Adapter {
    pub fn cells(
        &self,
        anchor: CellCoord,
    ) -> Result<Vec<(CellCoord, crate::world::BlockId)>, EntityError> {
        let mut cells = self
            .definition
            .footprint
            .iter()
            .map(|c| {
                Ok((
                    offset(anchor, c.offset)?,
                    self.catalog
                        .state_by_key(&c.state)
                        .ok_or(EntityError::InvalidType)?,
                ))
            })
            .collect::<Result<Vec<_>, EntityError>>()?;
        cells.sort_by_key(|(c, _)| *c);
        Ok(cells)
    }
    fn states(
        &self,
        s: &EntitySnapshot,
        remove: bool,
    ) -> Result<Vec<EntityBlockStateChange>, EntityError> {
        Ok(self
            .cells(s.anchor().ok_or(EntityError::WrongOwnership)?)?
            .into_iter()
            .map(|(cell, before)| EntityBlockStateChange {
                cell,
                before,
                after: if remove { crate::world::AIR } else { before },
            })
            .collect())
    }
}
fn offset(a: CellCoord, v: [i32; 3]) -> Result<CellCoord, EntityError> {
    Ok(CellCoord::new(
        a.x.checked_add(v[0]).ok_or(EntityError::InvalidLocation)?,
        a.y.checked_add(v[1]).ok_or(EntityError::InvalidLocation)?,
        a.z.checked_add(v[2]).ok_or(EntityError::InvalidLocation)?,
    ))
}
impl EntityPayloadCodec for Adapter {
    fn validate_location(&self, location: &EntityLocation) -> Result<(), EntityError> {
        let EntityLocation::Anchored {
            anchor,
            anchor_state,
            footprint,
        } = location
        else {
            return Err(EntityError::WrongOwnership);
        };
        if self.catalog.state_by_key(&self.definition.anchor_state) != Some(*anchor_state)
            || self
                .cells(*anchor)?
                .iter()
                .map(|(c, _)| *c)
                .ne(footprint.iter().copied())
        {
            return Err(EntityError::InvalidLocation);
        }
        Ok(())
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > self.definition.max_state_bytes {
            return Err(EntityCodecError::InvalidData);
        }
        let p = self
            .definition
            .behavior
            .decode(bytes)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if self.encode(&p)? != bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(p)
    }
    fn encode(&self, p: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bytes = self
            .definition
            .behavior
            .encode(p)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if bytes.len() > self.definition.max_state_bytes {
            return Err(EntityCodecError::InvalidData);
        }
        // Admission must enforce the same canonical round trip as recovery.
        // Otherwise a successful callback could journal an unrecoverable value.
        let restored = self
            .definition
            .behavior
            .decode(&bytes)
            .map_err(|_| EntityCodecError::InvalidData)?;
        let canonical = self
            .definition
            .behavior
            .encode(&restored)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if canonical != bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(bytes)
    }
    fn public_view(&self, p: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(p)?;
        let bytes = self
            .definition
            .behavior
            .public(p)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if bytes.len() > self.definition.max_public_bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(bytes)
    }
}
impl EntityTickPolicy for Adapter {
    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn wakes_on_terrain_change(&self) -> bool {
        true
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        _: &Catalog,
        view: &VoxelView,
        _: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let anchor = snapshot.anchor().ok_or(EntityError::WrongOwnership)?;
        let cells = self
            .definition
            .observe
            .iter()
            .map(|v| {
                let c = offset(anchor, *v)?;
                let block = view
                    .block(c.x, c.y, c.z)
                    .map_err(|_| EntityError::ViewOutOfRange)?;
                Ok(api::Cell {
                    offset: *v,
                    state: self
                        .catalog
                        .state(block)
                        .ok_or(EntityError::InvalidType)?
                        .key
                        .as_ref(),
                    solid: self.catalog.block_flags(block) & crate::content::SOLID != 0,
                })
            })
            .collect::<Result<Vec<_>, EntityError>>()?;
        let reaction = self
            .definition
            .behavior
            .react(&api::Context {
                anchor: [anchor.x, anchor.y, anchor.z],
                tick,
                state: &snapshot.private_payload,
                cells: &cells,
            })
            .map_err(|_| EntityError::InvalidPayload)?;
        let (payload, remove) = match reaction {
            api::Reaction::Keep => (None, false),
            api::Reaction::Update(p) => (Some(p), false),
            api::Reaction::Remove => (None, true),
        };
        if let Some(p) = &payload {
            self.public_view(p)
                .map_err(|_| EntityError::InvalidPayload)?;
        }
        // Early harmless hints retain the durable deadline. Due work advances
        // from current time; restart never replays callback invocation counts.
        let next_tick = if snapshot.next_tick.is_some_and(|due| due > tick) {
            snapshot.next_tick
        } else {
            Some(
                tick.checked_add(u64::from(self.definition.interval))
                    .ok_or(EntityError::RevisionExhausted)?,
            )
        };
        Ok(EntityTickPlan {
            lifecycle: bloxgloom_host_api::entity::Lifecycle {
                despawn: remove,
                spawns: vec![],
            },
            payload,
            next_tick,
            anchor_update: None,
            position: None,
            block_states: self.states(snapshot, remove)?,
            wakes: vec![],
            transfer: None,
        })
    }
}
impl EntityInteractionPolicy for Adapter {
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn plan(
        &self,
        s: &EntitySnapshot,
        request: &[u8],
        inventory: &crate::inventory::Inventory,
        _: &Catalog,
        _: &VoxelView,
        _: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        // Tag 4: identity + revision + opaque own-state request. A late request
        // cannot act on a replacement occupying the same footprint cell.
        if request.len() < 18
            || request[0] != 4
            || request[1..9] != s.id.get().to_le_bytes()
            || request[9..17] != s.revision.to_le_bytes()
        {
            return Err(EntityError::InvalidPayload);
        }
        let payload = self
            .definition
            .behavior
            .interact(&s.private_payload, &request[17..])
            .map_err(|_| EntityError::InvalidPayload)?;
        self.public_view(&payload)
            .map_err(|_| EntityError::InvalidPayload)?;
        let mut inventory = inventory.clone();
        inventory.revision = inventory
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        Ok(EntityInteractionPlan {
            payload,
            inventory,
            block_states: self.states(s, false)?,
            wakes: vec![],
        })
    }
}
pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: Arc<Catalog>,
    id: crate::content::EntityTypeId,
) -> Result<(), EntityError> {
    let definition = catalog
        .anchored_entity(id)
        .cloned()
        .ok_or(EntityError::InvalidType)?;
    let anchor = catalog
        .state_by_key(&definition.anchor_state)
        .ok_or(EntityError::InvalidType)?;
    let adapter = Arc::new(Adapter {
        definition: definition.clone(),
        catalog,
    });
    builder.register(EntityTypeRegistration {
        id,
        ownership: EntityOwnership::anchored(vec![anchor], definition.footprint.len()),
        tick_policy: TickPolicy::Interval(definition.interval),
        max_payload_bytes: definition.max_state_bytes,
        codec: adapter.clone(),
    })?;
    builder.register_tick_planner(id, adapter.clone())?;
    builder.register_interaction_policy(id, adapter)
}
