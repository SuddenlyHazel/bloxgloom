//! Anchored destruction inside the single action/tick decision overlay.
use super::*;
use bloxgloom_host_api::gameplay::{Context, Event, EventKind, Plan, cell_random};

#[derive(Default)]
pub(in crate::server) struct Expansion {
    pub snapshots: BTreeMap<entities::EntityId, EntitySnapshot>,
    pub cells: BTreeMap<[i32; 3], String>,
}

impl Expansion {
    /// Called before each neighbor frontier. The context's 4096-operation bound
    /// bounds capture even for rejected plans; retained expansion is at most 32
    /// entities / 256 cells. Presence AND absence enter the context read fence.
    pub(in crate::server) fn expand(
        &mut self,
        context: &mut Context<'_>,
        catalog: &std::sync::Arc<crate::content::Catalog>,
        store: &entities::EntityStore,
        lifecycles: &crate::server::lifecycle::Registry,
        seed: u64,
        tick: u64,
    ) -> io::Result<()> {
        let convert = crate::server::gameplay::error;
        loop {
            let transitions = context.staged_block_transitions();
            if transitions.len() > 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "expanded edit budget",
                ));
            }
            let mut callbacks = Vec::new();
            for (at, _, after) in &transitions {
                let after_id = catalog.state_by_key(&after.state).unwrap();
                if catalog.anchored_for_state(after_id).is_some()
                    || catalog.inventory_for_state(after_id).is_some()
                    || lifecycles.for_state(catalog, after_id).is_some()
                {
                    return Err(io::Error::new(
                        ErrorKind::PermissionDenied,
                        "gameplay cannot create an uninitialized anchored block",
                    ));
                }
                let Some(raw) = context.anchored_entity_at(*at).map_err(convert)? else {
                    continue;
                };
                let id = entities::EntityId::new(raw).unwrap();
                if self.snapshots.contains_key(&id) {
                    continue;
                }
                if self.snapshots.len() == 32 {
                    return Err(io::Error::new(
                        ErrorKind::QuotaExceeded,
                        "anchor invalidation budget",
                    ));
                }
                // The public read captures the full entity revision dependency.
                context.entity(raw).map_err(convert)?.ok_or_else(|| {
                    io::Error::new(ErrorKind::WouldBlock, "invalidated anchor disappeared")
                })?;
                let snapshot = store.snapshot(id).ok_or_else(|| {
                    io::Error::new(ErrorKind::WouldBlock, "invalidated anchor disappeared")
                })?;
                let EntityLocation::Anchored {
                    anchor,
                    ref footprint,
                    ..
                } = snapshot.location
                else {
                    return Err(io::Error::other("invalid anchored location"));
                };
                let touched = EntityCell::new(at[0], at[1], at[2]);
                let cells = removal_cells(catalog, lifecycles, &snapshot, Some(touched))?;
                let unique: BTreeSet<_> = cells.iter().map(|(cell, _)| *cell).collect();
                if cells.len() != footprint.len()
                    || unique.len() != cells.len()
                    || !unique.contains(&touched)
                    || !unique.contains(&anchor)
                    || unique.iter().any(|cell| !footprint.contains(cell))
                {
                    return Err(io::Error::other("registered footprint differs from entity"));
                }
                if self.cells.len() + cells.len() > 256 {
                    return Err(io::Error::new(
                        ErrorKind::QuotaExceeded,
                        "anchor footprint budget",
                    ));
                }
                let mut previous_anchor = None;
                for (cell, expected) in cells {
                    let at = [cell.x, cell.y, cell.z];
                    let current = context.block(at).map_err(convert)?;
                    let transition = transitions.iter().find(|(changed, _, _)| *changed == at);
                    let previous = transition.map_or(&current, |(_, previous, _)| previous);
                    if catalog.state_by_key(&previous.state) != Some(expected)
                        || context.anchored_entity_at(at).map_err(convert)? != Some(raw)
                    {
                        return Err(io::Error::new(
                            ErrorKind::WouldBlock,
                            "anchor footprint changed",
                        ));
                    }
                    if cell == anchor {
                        previous_anchor = Some(previous.clone());
                    }
                    let after = if transition.is_some() {
                        current.state
                    } else {
                        let air = catalog.state(AIR).unwrap().key.clone();
                        context.set_block(at, &air).map_err(convert)?;
                        air
                    };
                    self.cells.insert(at, after);
                }
                self.snapshots.insert(id, snapshot);
                callbacks.push(Event::BlockRemoved {
                    cell: [anchor.x, anchor.y, anchor.z],
                    previous: previous_anchor.expect("validated footprint includes anchor"),
                    cause: RemovalCause::AnchoredBreak,
                    random: cell_random(seed, [anchor.x, anchor.y, anchor.z], tick),
                });
            }
            if callbacks.is_empty() {
                return Ok(());
            }
            // Lifecycle owns the refund; one AnchoredBreak decision observes
            // the fully removed footprint, never one harvest per occupied cell.
            for event in callbacks {
                let Event::BlockRemoved { ref previous, .. } = event else {
                    unreachable!()
                };
                if let Some(handler) =
                    catalog.gameplay_handler(EventKind::BlockRemoved, &previous.block_type)
                {
                    context.dispatch(handler, &event).map_err(convert)?;
                }
            }
        }
    }

    pub(in crate::server) fn validate(&self, plan: &Plan) -> io::Result<()> {
        if self
            .cells
            .iter()
            .any(|(cell, after)| plan.blocks.get(cell) != Some(after))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "gameplay rewrote an invalidated footprint",
            ));
        }
        if self.snapshots.keys().any(|id| {
            plan.entity_changes.contains_key(&id.get())
                || plan.entity_schedules.contains_key(&id.get())
        }) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "invalidated anchor has conflicting entity effects",
            ));
        }
        Ok(())
    }

    /// Snapshots already contain the final validated inventory overlay. No
    /// inventory update is submitted for an entity being despawned, and refunds
    /// cannot reintroduce items transferred out during any decision callback.
    pub(in crate::server) fn finish(
        self,
        catalog: &std::sync::Arc<crate::content::Catalog>,
        lifecycles: &crate::server::lifecycle::Registry,
        store: &entities::EntityStore,
    ) -> io::Result<(
        Vec<crate::server::gameplay::Spawn>,
        Vec<entities::PreparedEntityTransaction>,
    )> {
        let mut drops = Vec::new();
        let mut despawns = Vec::new();
        for (id, snapshot) in self.snapshots {
            let stacks = removal_refunds(catalog, lifecycles, &snapshot)?;
            if drops.len() + stacks.len() > 1760 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "anchor refund budget",
                ));
            }
            let anchor = snapshot.anchor().unwrap();
            let position = [anchor.x, anchor.y, anchor.z].map(|n| n as f32 + 0.5);
            if !stacks.is_empty() {
                store
                    .capture_mobile_dependencies(position, 1.0)
                    .map_err(capacity_error)?;
            }
            drops.extend(
                stacks
                    .into_iter()
                    .map(|stack| (position, stack, Duration::from_millis(250))),
            );
            despawns.push(
                store
                    .prepare_despawn(id, snapshot.revision)
                    .map_err(capacity_error)?,
            );
        }
        Ok((drops, despawns))
    }
}
