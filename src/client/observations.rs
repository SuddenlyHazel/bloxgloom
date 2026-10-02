//! Bounded copies of accepted server data for presentation workers.
use super::*;
use presentation::{ActionView, BlockView, InventoryView, WorldView};

impl ClientApp {
    fn publish_observations(&mut self, event: &str, value: String) {
        if let Some(ui) = &mut self.package_ui {
            ui.observe(event, value.clone(), Arc::clone(&self.observations));
        }
        if let Some(visual) = &mut self.visual_session {
            visual.observe(event, value, Arc::clone(&self.observations));
        }
    }

    fn publish_extra_observations(&mut self, event: &str, value: String) {
        if let Some(ui) = &mut self.package_ui {
            ui.observe_extra(event, value.clone(), Arc::clone(&self.observations));
        }
        if let Some(visual) = &mut self.visual_session {
            visual.observe(event, value, Arc::clone(&self.observations));
        }
    }

    pub(super) fn observe_inventory(&mut self) {
        let inventory = match InventoryView::from_inventory(&self.inventory, &self.catalog) {
            Ok(inventory) => inventory,
            Err(error) => {
                self.fail_session(error);
                return;
            }
        };
        Arc::make_mut(&mut self.observations).inventory = Some(inventory);
        let items: usize = self
            .inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| usize::from(stack.count))
            .sum();
        self.publish_observations(
            "replica:inventory",
            format!("revision={};items={items}", self.inventory.revision),
        );
    }

    pub(super) fn observe_weather(&mut self, snapshot: crate::weather::WeatherSnapshot) {
        if !snapshot.valid()
            || self.observations.weather.is_some_and(|old| {
                snapshot.revision < old.revision || snapshot.elapsed_ms < old.elapsed_ms
            })
        {
            return;
        }
        Arc::make_mut(&mut self.observations).weather =
            Some(snapshot.observation(snapshot.elapsed_ms));
        self.publish_extra_observations("replica:weather", "weather=updated".into());
    }

    pub(super) fn observe_time(&mut self, elapsed_ms: u64) {
        Arc::make_mut(&mut self.observations).world = Some(WorldView {
            elapsed_ms,
            cycle_ms: crate::daylight::CYCLE_MS,
        });
        self.publish_extra_observations("replica:world", "clock=updated".into());
    }

    pub(super) fn observe_action_spawns(
        &mut self,
        id: u128,
        spawned: Vec<crate::protocol::SpawnReceipt>,
    ) {
        if crate::protocol::action_spawns::validate(&spawned).is_err() {
            self.fail_session("invalid action spawn mapping");
            return;
        }
        if let Some(action) = self
            .observations
            .actions
            .iter()
            .find(|action| action.id == id)
        {
            if action.spawned != spawned {
                self.fail_session("replayed spawn mapping differs");
            }
            return;
        }
        if !self.pending_actions.contains_key(&id) {
            return;
        }
        let observations = Arc::make_mut(&mut self.observations);
        if let Some(old) = observations.pending_spawns.get(&id) {
            if old != &spawned {
                self.fail_session("duplicate spawn mapping differs");
            }
            return;
        }
        if observations.pending_spawns.len() >= 64 {
            self.fail_session("action spawn mapping capacity");
            return;
        }
        observations.pending_spawns.insert(id, spawned);
    }

    pub(super) fn observe_action(&mut self, id: u128, accepted: bool, reason: &str) {
        if self
            .observations
            .actions
            .iter()
            .any(|receipt| receipt.id == id)
        {
            return; // Retry delivery is not a new terminal observation.
        }
        let snapshot = Arc::make_mut(&mut self.observations);
        let key = self
            .pending_actions
            .get(&id)
            .and_then(|message| match message {
                ClientMessage::EntityInteract { payload, .. } => {
                    bloxgloom_host_api::actions::Request::decode(payload)
                        .or_else(|| {
                            bloxgloom_host_api::actions::TerrainRequest::decode(payload)
                                .map(|wrapped| wrapped.request)
                        })
                        .map(|request| request.key)
                }
                _ => None,
            });
        if snapshot.actions.len() == 16 {
            snapshot.actions.remove(0);
        }
        let spawned = snapshot.pending_spawns.remove(&id).unwrap_or_default();
        if !accepted && !spawned.is_empty() {
            self.fail_session("rejected action carried committed spawns");
            return;
        }
        snapshot.actions.push(ActionView {
            spawned,
            id,
            key,
            accepted,
            reason: reason.to_owned(),
        });
        self.publish_observations("replica:action", format!("accepted={accepted}"));
    }

    pub(super) fn observe_installed_blocks(
        &mut self,
        keys: &[ChunkKey],
        cells: Vec<(ChunkKey, [u8; 3])>,
        truncated: bool,
        summary: Option<String>,
    ) {
        if keys.is_empty() && cells.is_empty() && !truncated {
            return;
        }
        let snapshot = Arc::make_mut(&mut self.observations);
        // Snapshot/resync installation refreshes any cells retained from an older revision.
        for view in &mut snapshot.blocks {
            let (key, local) =
                crate::world::world_to_chunk(view.position[0], view.position[1], view.position[2]);
            if keys.contains(&key)
                && let Some(chunk) = self.chunks.get(&key)
                && let Some(block) = chunk.block(local)
                && let Some(state) = self.catalog.state(block)
            {
                view.state = state.key.to_string();
                view.version = chunk.version;
            }
        }
        snapshot.blocks_truncated |= truncated;
        for (key, local) in cells {
            let Some(chunk) = self.chunks.get(&key) else {
                continue;
            };
            let Some(block) = chunk.block(local.map(usize::from)) else {
                continue;
            };
            let Some(state) = self.catalog.state(block) else {
                continue;
            };
            let coordinates = [key.x, key.y, key.z];
            let mut position = [0; 3];
            let mut valid = true;
            for axis in 0..3 {
                match coordinates[axis]
                    .checked_mul(crate::world::CHUNK_SIZE as i32)
                    .and_then(|value| value.checked_add(i32::from(local[axis])))
                {
                    Some(value) => position[axis] = value,
                    None => valid = false,
                }
            }
            if !valid {
                snapshot.blocks_truncated = true;
                continue;
            }
            snapshot.blocks.retain(|view| view.position != position);
            if snapshot.blocks.len() == 64 {
                snapshot.blocks.remove(0);
                snapshot.blocks_truncated = true;
            }
            snapshot.blocks.push(BlockView {
                position,
                state: state.key.to_string(),
                version: chunk.version,
            });
        }
        if !keys.is_empty() || truncated {
            if let Some(summary) = summary {
                self.publish_observations("replica:block", summary);
            } else {
                self.publish_extra_observations(
                    "replica:block",
                    format!("changed_chunks={}", keys.len()),
                );
            }
        }
    }

    pub(super) fn prune_observed_blocks(&mut self) {
        if self.observations.blocks.iter().all(|view| {
            self.chunks.contains_key(
                &crate::world::world_to_chunk(view.position[0], view.position[1], view.position[2])
                    .0,
            )
        }) {
            return;
        }
        let snapshot = Arc::make_mut(&mut self.observations);
        let before = snapshot.blocks.len();
        snapshot.blocks.retain(|view| {
            self.chunks.contains_key(
                &crate::world::world_to_chunk(view.position[0], view.position[1], view.position[2])
                    .0,
            )
        });
        if before != snapshot.blocks.len() {
            snapshot.blocks_truncated = true;
            self.publish_extra_observations("replica:block", "evicted=true".into());
        }
    }
}

#[cfg(test)]
mod tests;
