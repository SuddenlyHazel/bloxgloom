//! A small terrain-only wanderer. All decisions are pure worker plans; the
//! shared entity coordinator owns scheduling, read fences, WAL and publication.
//!
//! Walking/settling stays due every four logical ticks; idle waits 40..=80
//! ticks (0.8..=1.6 seconds at 50 Hz). This autonomous timer never suspends:
//! the bounded suspended-recheck lane is for dependency sleepers, not an AI
//! clock. Missing terrain retains the ordinary durable due entry and retries
//! through the existing fair admission lane; no catch-up steps are simulated.
use super::*;
use crate::content::Catalog;
use crate::server::voxel_view::VoxelView;
use std::sync::Arc;

mod terrain;
pub(in crate::server) use terrain::spawn_clear;

const STEP_TICKS: u64 = 4;
const STEP: f32 = 0.125;

/// Counter-based choices survive restart and do not depend on receipt latency.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::server) struct Mossbun {
    pub cycle: u64,
    pub facing: u8,
    pub steps: u8,
}

struct Codec;
impl EntityPayloadCodec for Codec {
    fn validate_location(&self, location: &EntityLocation) -> Result<(), EntityError> {
        match location {
            EntityLocation::Mobile { position } if terrain::valid_position(*position) => Ok(()),
            _ => Err(EntityError::InvalidLocation),
        }
    }

    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() != 10 || bytes[8] > 3 || bytes[9] > 16 {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(Mossbun {
            cycle: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            facing: bytes[8],
            steps: bytes[9],
        }))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bun = payload
            .downcast_ref::<Mossbun>()
            .ok_or(EntityCodecError::InvalidData)?;
        if bun.facing > 3 || bun.steps > 16 {
            return Err(EntityCodecError::InvalidData);
        }
        let mut bytes = bun.cycle.to_le_bytes().to_vec();
        bytes.extend([bun.facing, bun.steps]);
        Ok(bytes)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(payload)?;
        let bun = payload.downcast_ref::<Mossbun>().unwrap();
        Ok(vec![bun.facing, u8::from(bun.steps > 0)])
    }
}

pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Catalog,
) -> Result<(), EntityError> {
    let id = catalog
        .entity_type_id_by_key("bloxgloom:mossbun")
        .ok_or(EntityError::InvalidType)?;
    builder.register(EntityTypeRegistration {
        id,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(STEP_TICKS as u32),
        max_payload_bytes: 10,
        codec: Arc::new(Codec),
    })?;
    builder.register_tick_planner(id, Arc::new(Wander))
}

struct Wander;
impl EntityTickPolicy for Wander {
    fn wakes_on_terrain_change(&self) -> bool {
        true
    }

    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn reads_neighbours(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        _catalog: &Catalog,
        view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let mut plan = EntityTickPlan {
            payload: None,
            next_tick: snapshot.next_tick,
            anchor_update: None,
            position: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        };
        let EntityLocation::Mobile { position } = snapshot.location else {
            return Err(EntityError::InvalidLocation);
        };
        if !terrain::valid_position(position) {
            return Err(EntityError::InvalidLocation);
        }
        // A harmless early hint must not advance AI, but loss of support
        // interrupts idle immediately rather than waiting for the AI deadline.
        let clear = terrain::clear(view, position)?;
        let grounded = terrain::grounded(view, position)?;
        if snapshot.next_tick.is_some_and(|due| due > tick) && (!clear || grounded) {
            return Ok(plan);
        }
        let mut bun = *snapshot
            .private_payload
            .downcast_ref::<Mossbun>()
            .ok_or(EntityError::InvalidPayload)?;
        let mut next = position;
        let mut delay = STEP_TICKS;
        // Never tunnel out of a newly placed block; wait for it to be removed.
        if !clear {
            bun.steps = 0;
            delay = 40;
        } else if !grounded {
            // Bounded settling gravity: at most 1/4 block, snapping to the top
            // of a solid voxel rather than accumulating sub-voxel penetration.
            next[1] -= 0.25;
            if !terrain::clear(view, next)? {
                next[1] = position[1].floor();
            }
            if !terrain::valid_position(next) || !terrain::clear(view, next)? {
                next = position;
            }
        } else if bun.steps == 0 {
            bun.cycle = bun.cycle.wrapping_add(1);
            let choice = mix(snapshot.id.get() ^ mix(bun.cycle));
            bun.facing = (choice & 3) as u8;
            bun.steps = 8 + ((choice >> 8) % 9) as u8;
        } else {
            let [dx, dz] =
                [[0.0, STEP], [STEP, 0.0], [0.0, -STEP], [-STEP, 0.0]][usize::from(bun.facing)];
            next[0] += dx;
            next[2] += dz;
            // All four corners must have support: no stepping off edges, no
            // jumping or automatic climbing. Missing terrain is never air.
            if !terrain::valid_position(next)
                || !terrain::clear(view, next)?
                || !terrain::supported(view, next)?
            {
                next = position;
                bun.steps = 0;
            } else {
                bun.steps -= 1;
            }
            if bun.steps == 0 {
                delay = 40 + mix(bun.cycle ^ snapshot.id.get()) % 41;
            }
        }
        plan.next_tick = Some(
            tick.checked_add(delay)
                .ok_or(EntityError::RevisionExhausted)?,
        );
        plan.payload = Some(EntityPayload::new(bun));
        plan.position = (next != position).then_some(next);
        Ok(plan)
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
