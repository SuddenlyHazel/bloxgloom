//! Best-effort, bounded post-WAL observer delivery. Authoritative decisions
//! never wait for advisory callbacks, and replay never impersonates a live event.
use super::entities::{EntityCommit, EntityDelta, EntityLocation, EntityPublicView};
use crate::{content::Catalog, inventory::Inventory, server::durable::BlockDelta};
use bloxgloom_host_api::gameplay::{
    Committed, CommittedBlock, CommittedEntity, Entity, WeatherChanged,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{self, SyncSender},
    thread,
};

const QUEUE: usize = 32;
const MAX_EVENT_BYTES: usize = 256 * 1024;

enum Event {
    Committed(Committed),
    Weather(WeatherChanged),
}
pub(super) struct Lane(Option<SyncSender<Event>>);

impl Lane {
    pub(super) fn new(catalog: &Catalog) -> std::io::Result<Self> {
        let observers: Vec<_> = catalog.gameplay_observers().cloned().collect();
        if observers.is_empty() {
            return Ok(Self(None));
        }
        let (sender, receiver) = mpsc::sync_channel::<Event>(QUEUE);
        thread::Builder::new()
            .name("gameplay-observers".into())
            .spawn(move || {
                let mut active = observers
                    .into_iter()
                    .map(|observer| (observer, true))
                    .collect::<Vec<_>>();
                while let Ok(event) = receiver.recv() {
                    for (registration, enabled) in &mut active {
                        if *enabled
                            && catch_unwind(AssertUnwindSafe(|| match &event {
                                Event::Committed(event) => registration.observer.on_commit(event),
                                Event::Weather(event) => registration.observer.on_weather(event),
                            }))
                            .is_err()
                        {
                            // One faulty observer cannot kill the delivery lane or
                            // stall unrelated observers with repeated panics.
                            *enabled = false;
                        }
                    }
                }
            })?;
        Ok(Self(Some(sender)))
    }

    pub(super) fn weather(
        &self,
        previous: crate::weather::WeatherSnapshot,
        current: crate::weather::WeatherSnapshot,
    ) {
        if (previous.revision, previous.to, previous.transition_start_ms)
            == (current.revision, current.to, current.transition_start_ms)
        {
            return;
        }
        if let Some(sender) = &self.0 {
            let _ = sender.try_send(Event::Weather(WeatherChanged {
                previous: previous.observation(previous.elapsed_ms),
                current: current.observation(current.elapsed_ms),
            }));
        }
    }

    pub(super) fn enqueue(
        &self,
        catalog: &Catalog,
        blocks: &[BlockDelta],
        entities: Option<&EntityCommit>,
        profile: Option<u128>,
        inventory: Option<&Inventory>,
    ) {
        let Some(sender) = &self.0 else {
            return;
        };
        let mut size = 0usize;
        let mut public_blocks = Vec::with_capacity(blocks.len());
        for delta in blocks {
            let mut cell = [0_i32; 3];
            for (axis, (chunk, local)) in [delta.key.x, delta.key.y, delta.key.z]
                .into_iter()
                .zip(delta.local)
                .enumerate()
            {
                let world = i64::from(chunk) * crate::world::CHUNK_SIZE as i64 + i64::from(local);
                let Ok(value) = i32::try_from(world) else {
                    return;
                };
                cell[axis] = value;
            }
            let Some(state) = catalog.state(delta.block) else {
                return;
            };
            size += state.key.len() + 12;
            if size > MAX_EVENT_BYTES {
                return;
            }
            public_blocks.push(CommittedBlock {
                cell,
                state: state.key.clone(),
            });
        }
        let mut public_entities = Vec::new();
        if let Some(commit) = entities {
            for delta in &commit.deltas {
                let change = match delta {
                    EntityDelta::Spawned(view) => {
                        let Some(entity) = view_entity(catalog, view) else {
                            return;
                        };
                        CommittedEntity::Spawned(entity)
                    }
                    EntityDelta::Updated { view, .. }
                    | EntityDelta::Transferred { view, .. }
                    | EntityDelta::Moved(view) => {
                        let Some(entity) = view_entity(catalog, view) else {
                            return;
                        };
                        CommittedEntity::Updated(entity)
                    }
                    EntityDelta::Despawned {
                        id, entity_type, ..
                    } => {
                        let Some(kind) = catalog.entity_type(*entity_type) else {
                            return;
                        };
                        CommittedEntity::Removed {
                            id: id.get(),
                            key: kind.key.to_string(),
                        }
                    }
                };
                size += match &change {
                    CommittedEntity::Spawned(entity) | CommittedEntity::Updated(entity) => {
                        entity.data.len() + entity.entity_type.len() + 32
                    }
                    CommittedEntity::Removed { key, .. } => key.len() + 8,
                };
                if size > MAX_EVENT_BYTES {
                    return;
                }
                public_entities.push(change);
            }
        }
        let inventory = profile.zip(inventory.map(|value| value.revision));
        if public_blocks.is_empty() && public_entities.is_empty() && inventory.is_none() {
            return;
        }
        // Overflow/disconnect affects only advisory observations. The WAL and
        // replicated committed state have already been applied at this point.
        let _ = sender.try_send(Event::Committed(Committed {
            blocks: public_blocks,
            entities: public_entities,
            inventory,
        }));
    }
}

fn view_entity(catalog: &Catalog, view: &EntityPublicView) -> Option<Entity> {
    let key = catalog.entity_type(view.entity_type)?.key.to_string();
    let (position, anchor) = match &view.location {
        EntityLocation::Mobile { position } => (*position, None),
        EntityLocation::Anchored { anchor, .. } => {
            let cell = [anchor.x, anchor.y, anchor.z];
            (cell.map(|n| n as f32 + 0.5), Some(cell))
        }
    };
    Some(Entity {
        id: view.id.get(),
        entity_type: key,
        position,
        anchor,
        data: view.payload.clone(),
    })
}
