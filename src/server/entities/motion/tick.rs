//! Fixed logical cadence and persisted pending reactions share entity/WAL ownership.
use super::{
    DT, MAX_CHUNK_BODIES, MAX_COLLIDERS, MAX_DYNAMIC_COLLIDERS, MAX_SWEEP_CELLS, STEP_TICKS, solver,
};
use crate::server::{
    State,
    durable::{CommitAction, TerrainReads},
    entities::{EntityId, EntityPatch, EntityPayload},
};
use bloxgloom_host_api::{
    gameplay::Event,
    motion::{ExpiryReason, Impact, Pending, Record, Target},
};
use std::io::{self, ErrorKind};

pub(in crate::server) fn plan(
    state: &mut State,
    id: EntityId,
    tick: u64,
) -> io::Result<Option<CommitAction>> {
    let Some(snapshot) = state.entities.snapshot(id) else {
        return Ok(None);
    };
    if snapshot.next_tick.is_none_or(|due| due > tick) {
        return Ok(None);
    }
    let catalog = state.world.catalog_arc();
    let declaration = catalog
        .moving_entity(snapshot.entity_type)
        .cloned()
        .ok_or_else(|| invalid("moving declaration missing"))?;
    let bytes = snapshot
        .private_payload
        .downcast_ref::<Vec<u8>>()
        .ok_or_else(|| invalid("moving record missing"))?;
    let mut record = Record::decode(bytes).map_err(|e| invalid(&e.0))?;
    if let Some(pending) = record.pending.clone() {
        let event = match pending {
            Pending::Impact(impact) => Event::MovingImpact { impact },
            Pending::Expiry {
                entity,
                motion_revision,
                tick,
                reason,
            } => Event::MovingExpiry {
                entity,
                motion_revision,
                tick,
                reason,
            },
        };
        return reaction(state, id, tick, event);
    }
    if record.remaining_ticks == 0 {
        let mut reads = TerrainReads::default();
        reads.entities(state.entities.capture_entity_dependency(id))?;
        return expire(
            state,
            id,
            snapshot.revision,
            tick,
            record,
            ExpiryReason::Lifetime,
            reads,
        );
    }
    if record
        .next_behavior_tick
        .is_some_and(|due| due <= record.simulation_tick)
    {
        return reaction(
            state,
            id,
            tick,
            Event::MovingTick {
                entity: id.get(),
                tick,
                motion: record.motion,
            },
        );
    }
    // Dormant owner chunks do not generate an unbounded travelling frontier.
    let owner = crate::world::world_to_chunk(
        record.motion.position[0].floor() as i32,
        record.motion.position[1].floor() as i32,
        record.motion.position[2].floor() as i32,
    )
    .0;
    if !state
        .clients
        .values()
        .any(|client| client.interested(owner))
    {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "moving owner is dormant",
        ));
    }
    let mut reads = TerrainReads::default();
    reads.entities(state.entities.capture_entity_dependency(id))?;
    let mut acceleration = record.motion.acceleration.map(f64::from);
    acceleration[1] -= 20.0 * f64::from(declaration.body.gravity_scale);
    let mut velocity = record.motion.velocity.map(f64::from);
    // Apply acceleration once, then cap the integrated velocity before sweep.
    for axis in 0..3 {
        velocity[axis] += acceleration[axis] * DT;
    }
    clamp(&mut velocity, f64::from(declaration.body.max_speed));
    let half = declaration.body.half_extents.map(f64::from);
    let displacement = velocity.map(|v| v * DT);
    // Reflected paths stay within a conservative captured envelope. Dynamic
    // responses can add speed, so their envelope uses the declared speed cap.
    let bounds = if declaration.body.response == bloxgloom_host_api::motion::Response::Stop {
        solver::swept_cells(
            record.motion.position.map(f64::from),
            displacement,
            half,
            MAX_SWEEP_CELLS,
        )
    } else {
        let speed = if declaration.body.collisions.players || declaration.body.collisions.creatures
        {
            f64::from(declaration.body.max_speed)
        } else {
            velocity.iter().map(|v| v * v).sum::<f64>().sqrt()
        };
        solver::swept_cells(
            record.motion.position.map(f64::from),
            [0.0; 3],
            half.map(|h| h + speed * DT),
            MAX_SWEEP_CELLS,
        )
    }
    .map_err(solver_error)?;
    let previous =
        record
            .contact
            .as_ref()
            .zip(record.contact_normal)
            .and_then(|(target, normal)| {
                let target = match target {
                    Target::Terrain { cell, state } => solver::Target::Terrain {
                        cell: *cell,
                        state: catalog.state_by_key(state)?.0,
                    },
                    Target::Entity { id, revision } => {
                        if id & (1u64 << 63) != 0 {
                            solver::Target::Player {
                                id: *id,
                                revision: *revision,
                            }
                        } else {
                            solver::Target::Creature {
                                id: *id,
                                revision: *revision,
                            }
                        }
                    }
                };
                Some(solver::ContactMemory {
                    target,
                    normal: normal.map(f64::from),
                })
            });
    let mut colliders = Vec::new();
    let mut missing = std::collections::BTreeSet::new();
    for x in bounds.0[0]..=bounds.1[0] {
        for y in bounds.0[1]..=bounds.1[1] {
            for z in bounds.0[2]..=bounds.1[2] {
                let Some(block) = reads.read(&mut state.world, x, y, z)? else {
                    missing.insert(crate::world::world_to_chunk(x, y, z).0);
                    continue;
                };
                if declaration.body.collisions.terrain
                    && catalog.block_flags(block) & crate::content::SOLID != 0
                {
                    colliders.push(solver::Collider {
                        target: solver::Target::Terrain {
                            cell: [x, y, z],
                            state: block.0,
                        },
                        min: [x as f64, y as f64, z as f64],
                        max: [x as f64 + 1.0, y as f64 + 1.0, z as f64 + 1.0],
                        displacement: [0.0; 3],
                    });
                }
            }
        }
    }
    if !missing.is_empty() {
        for key in missing {
            let _ = crate::server::streaming::request_chunk(state, key);
        }
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "moving sweep terrain unavailable",
        ));
    }
    colliders.extend(super::colliders::capture(
        state,
        id,
        tick,
        &declaration,
        &record,
        bounds,
        &mut reads,
    )?);
    let terrain_count = colliders
        .iter()
        .filter(|c| matches!(c.target, solver::Target::Terrain { .. }))
        .count();
    if colliders.len() - terrain_count > MAX_DYNAMIC_COLLIDERS {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "moving collider capture capacity",
        ));
    }
    let step = match solver::integrate_with_contact_policy(
        solver::State {
            position: record.motion.position.map(f64::from),
            velocity,
            acceleration: [0.0; 3],
        },
        solver::Body {
            half_extents: half,
            response: match declaration.body.response {
                bloxgloom_host_api::motion::Response::Stop => solver::Response::Stop,
                bloxgloom_host_api::motion::Response::Bounce => solver::Response::Bounce,
                bloxgloom_host_api::motion::Response::Slide => solver::Response::Slide,
            },
            restitution: f64::from(declaration.body.restitution),
        },
        DT,
        &colliders,
        solver::Limits {
            colliders: MAX_COLLIDERS,
            sweep_cells: MAX_SWEEP_CELLS,
            contacts: 4,
            world_min: [
                -999_998.0,
                f64::from(crate::world::BEDROCK_Y) + half[1] + 0.0001,
                -999_998.0,
            ],
            world_max: [999_998.0; 3],
        },
        solver::ContactPolicy {
            previous,
            pause_on_new: declaration.handles_impact,
            max_speed: f64::from(declaration.body.max_speed),
        },
    ) {
        Ok(step) => step,
        Err(solver::Error::WorldBoundary) => {
            return expire(
                state,
                id,
                snapshot.revision,
                tick,
                record,
                ExpiryReason::WorldBoundary,
                reads,
            );
        }
        Err(error) => return Err(solver_error(error)),
    };
    record.motion.position = step.position.map(|x| x as f32);
    // Persisting a rounded center inside a touched face would turn ordinary
    // resting contact into a false embedded-body failure on the next step.
    if step.blocked != Some(solver::Blocked::Embedded) {
        if let Some(resting) = step.resting {
            for axis in 0..3 {
                let rounded = &mut record.motion.position[axis];
                if resting.normal[axis] > 0.0 && f64::from(*rounded) < step.position[axis] {
                    *rounded = rounded.next_up();
                }
                if resting.normal[axis] < 0.0 && f64::from(*rounded) > step.position[axis] {
                    *rounded = rounded.next_down();
                }
            }
        }
    }
    let mut v = step.velocity;
    clamp(&mut v, f64::from(declaration.body.max_speed));
    record.motion.velocity = v.map(|x| x as f32);
    record.motion.grounded = step.grounded;
    record.motion.revision = record
        .motion
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("moving revision exhausted"))?;
    record.simulation_tick = tick;
    record.remaining_ticks = record.remaining_ticks.saturating_sub(STEP_TICKS as u32);
    record.source_ticks = record.source_ticks.saturating_sub(STEP_TICKS as u32);
    let target_for = |target: solver::Target| -> io::Result<Target> {
        Ok(match target {
            solver::Target::Terrain { cell, state } => Target::Terrain {
                cell,
                state: catalog
                    .state(crate::content::BlockStateId(state))
                    .ok_or_else(|| invalid("collision state missing"))?
                    .key
                    .to_string(),
            },
            solver::Target::Player { id, revision } | solver::Target::Creature { id, revision } => {
                Target::Entity { id, revision }
            }
        })
    };
    record.contact = step.resting.map(|c| target_for(c.target)).transpose()?;
    record.contact_normal = step.resting.map(|c| c.normal.map(|v| v as f32));
    if declaration.handles_impact {
        if let Some(contact) = step
            .contacts
            .iter()
            .find(|c| !previous.is_some_and(|old| old.matches(c)))
        {
            record.pending = Some(Pending::Impact(Impact {
                entity: id.get(),
                motion_revision: record.motion.revision,
                tick,
                position: std::array::from_fn(|i| {
                    (contact.position[i] - contact.normal[i] * half[i]) as f32
                }),
                normal: contact.normal.map(|x| x as f32),
                incoming_velocity: contact.incoming_velocity.map(|x| x as f32),
                target: target_for(contact.target)?,
                blocked: step.blocked.is_some(),
            }));
        }
    }
    if record.pending.is_none() && record.remaining_ticks == 0 {
        if declaration.handles_expiry {
            record.pending = Some(Pending::Expiry {
                entity: id.get(),
                motion_revision: record.motion.revision,
                tick,
                reason: ExpiryReason::Lifetime,
            });
        } else {
            let transaction = state
                .entities
                .prepare_despawn(id, snapshot.revision)
                .map_err(io::Error::other)?;
            return Ok(Some(commit(reads, transaction)));
        }
    }
    let destination = crate::world::world_to_chunk(
        record.motion.position[0].floor() as i32,
        record.motion.position[1].floor() as i32,
        record.motion.position[2].floor() as i32,
    )
    .0;
    if destination != owner
        && state
            .entities
            .ids_for_chunk(destination)
            .into_iter()
            .filter(|id| {
                state
                    .entities
                    .snapshot(*id)
                    .is_some_and(|s| catalog.moving_entity(s.entity_type).is_some())
            })
            .count()
            >= MAX_CHUNK_BODIES
    {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "moving destination capacity",
        ));
    }
    let transaction = state
        .entities
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: Some(EntityPayload::new(
                    record.encode().map_err(|e| invalid(&e.0))?,
                )),
                position: Some(record.motion.position),
                next_tick: Some(Some(
                    if record.pending.is_some()
                        || record
                            .next_behavior_tick
                            .is_some_and(|due| due <= record.simulation_tick)
                    {
                        tick.saturating_add(1)
                    } else {
                        tick.saturating_add(STEP_TICKS)
                    },
                )),
            },
        )
        .map_err(io::Error::other)?;
    Ok(Some(commit(reads, transaction)))
}
fn clamp(v: &mut [f64; 3], max: f64) {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length > max {
        for x in v {
            *x *= max / length;
        }
    }
}
fn solver_error(error: solver::Error) -> io::Error {
    io::Error::new(
        match error {
            solver::Error::ColliderCapacity | solver::Error::SweepCapacity => {
                ErrorKind::QuotaExceeded
            }
            solver::Error::WorldBoundary | solver::Error::InvalidInput => ErrorKind::InvalidInput,
        },
        format!("moving sweep rejected: {error:?}"),
    )
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.to_owned())
}
fn commit(
    reads: TerrainReads,
    entities: crate::server::entities::PreparedEntityTransaction,
) -> CommitAction {
    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        entities: Some(entities),
        entity_wakes: vec![],
        owner_changes: vec![],
        player_publication: None,
    }
}
fn reaction(
    state: &mut State,
    id: EntityId,
    tick: u64,
    event: Event,
) -> io::Result<Option<CommitAction>> {
    crate::server::durable::actions::gameplay_tick::plan_event(state,id,tick,event).map_err(|error| {
        if error.kind()==ErrorKind::InvalidInput || error.kind()==ErrorKind::PermissionDenied {
            tracing::warn!(entity=id.get(),%error,"moving reaction rejected; record remains pending");
            io::Error::new(ErrorKind::QuotaExceeded,error)
        } else {error}
    })
}

fn expire(
    state: &mut State,
    id: EntityId,
    revision: u64,
    tick: u64,
    mut record: Record,
    reason: ExpiryReason,
    reads: TerrainReads,
) -> io::Result<Option<CommitAction>> {
    let snapshot = state
        .entities
        .snapshot(id)
        .ok_or_else(|| invalid("expiry entity disappeared"))?;
    let handles = state
        .world
        .catalog()
        .moving_entity(snapshot.entity_type)
        .ok_or_else(|| invalid("expiry declaration missing"))?
        .handles_expiry;
    let transaction = if handles {
        record.motion.revision = record
            .motion
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("moving revision exhausted"))?;
        record.simulation_tick = tick;
        record.pending = Some(Pending::Expiry {
            entity: id.get(),
            motion_revision: record.motion.revision,
            tick,
            reason,
        });
        state.entities.prepare_update(
            id,
            revision,
            EntityPatch {
                payload: Some(EntityPayload::new(
                    record.encode().map_err(|e| invalid(&e.0))?,
                )),
                position: None,
                next_tick: Some(Some(tick.saturating_add(1))),
            },
        )
    } else {
        state.entities.prepare_despawn(id, revision)
    }
    .map_err(io::Error::other)?;
    Ok(Some(commit(reads, transaction)))
}
