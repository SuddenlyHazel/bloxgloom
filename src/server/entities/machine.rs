//! Host-owned machine state and adapters. Extension hooks declare bounded work;
//! the codec, slot operations, process kernel, and transfer commit retain authority.
use super::*;
use crate::{
    content::{Catalog, EntityTypeId},
    inventory::Stack,
};
use bloxgloom_host_api::machine as api;
use std::sync::Arc;
mod automation;
mod codec;
mod inventory;
mod planning;
#[cfg(test)]
mod tests;
mod transaction;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::server) struct MachinePayload {
    pub variant: u8,
    pub fuel: u16,
    pub progress: u16,
    pub progress_item: Option<crate::items::ItemId>,
    pub slots: Vec<Option<Stack>>,
    pub data: Vec<u8>,
}
impl MachinePayload {
    pub fn empty(slots: u8, variant: u8) -> Self {
        Self {
            variant,
            fuel: 0,
            progress: 0,
            progress_item: None,
            slots: vec![None; usize::from(slots)],
            data: vec![],
        }
    }
}
#[derive(Clone)]
pub(in crate::server) struct Adapter {
    pub definition: Arc<api::Machine>,
    pub catalog: Arc<Catalog>,
    port: Option<u8>,
    slot: Option<u8>,
    lookups: Arc<Lookups>,
}
struct Lookups {
    items: std::collections::HashMap<String, crate::items::ItemId>,
    filters: Vec<std::collections::HashSet<crate::items::ItemId>>,
    recipes: std::collections::HashMap<crate::items::ItemId, Vec<usize>>,
    fuels: std::collections::HashMap<crate::items::ItemId, Vec<usize>>,
}
impl Adapter {
    pub fn new(catalog: Arc<Catalog>, definition: Arc<api::Machine>) -> Self {
        let items = catalog
            .items()
            .map(|i| (i.key.to_string(), i.id))
            .collect::<std::collections::HashMap<_, _>>();
        let filters = definition
            .filters
            .iter()
            .map(|f| {
                f.items
                    .iter()
                    .filter_map(|key| items.get(key).copied())
                    .collect()
            })
            .collect();
        let mut recipes: std::collections::HashMap<_, Vec<_>> = Default::default();
        for (i, r) in definition
            .process
            .iter()
            .flat_map(|p| p.recipes.iter().enumerate())
        {
            recipes.entry(items[&r.input]).or_default().push(i);
        }
        let mut fuels: std::collections::HashMap<_, Vec<_>> = Default::default();
        for (i, f) in definition
            .process
            .iter()
            .flat_map(|p| p.fuels.iter().enumerate())
        {
            fuels.entry(items[&f.item]).or_default().push(i);
        }
        Self {
            definition,
            catalog,
            port: None,
            slot: None,
            lookups: Arc::new(Lookups {
                items,
                filters,
                recipes,
                fuels,
            }),
        }
    }
    fn active(&self, p: &MachinePayload) -> bool {
        p.fuel > 0
    }
    pub fn cells(
        &self,
        anchor: CellCoord,
        p: &MachinePayload,
    ) -> Result<Vec<(CellCoord, crate::content::BlockStateId)>, EntityError> {
        let v = self
            .definition
            .variants
            .get(p.variant as usize)
            .ok_or(EntityError::InvalidPayload)?;
        let cells = if self.active(p) { &v.active } else { &v.idle };
        let mut result = Vec::new();
        for c in cells {
            let cell = CellCoord::new(
                anchor
                    .x
                    .checked_add(c.offset[0])
                    .ok_or(EntityError::InvalidLocation)?,
                anchor
                    .y
                    .checked_add(c.offset[1])
                    .ok_or(EntityError::InvalidLocation)?,
                anchor
                    .z
                    .checked_add(c.offset[2])
                    .ok_or(EntityError::InvalidLocation)?,
            );
            result.push((
                cell,
                self.catalog
                    .state_by_key(&c.state)
                    .ok_or(EntityError::InvalidType)?,
            ));
        }
        result.sort_by_key(|(c, _)| *c);
        Ok(result)
    }
    fn block_states(
        &self,
        s: &EntitySnapshot,
        before: &MachinePayload,
        after: &MachinePayload,
    ) -> Result<Vec<EntityBlockStateChange>, EntityError> {
        let anchor = s.anchor().ok_or(EntityError::WrongOwnership)?;
        Ok(self
            .cells(anchor, before)?
            .into_iter()
            .zip(self.cells(anchor, after)?)
            .map(|((cell, before), (_, after))| EntityBlockStateChange {
                cell,
                before,
                after,
            })
            .collect())
    }
    fn item(&self, key: &str) -> Result<crate::items::ItemId, EntityError> {
        self.lookups
            .items
            .get(key)
            .copied()
            .ok_or(EntityError::InvalidType)
    }
    fn recipe(&self, stack: &Stack) -> Option<&api::Recipe> {
        let process = self.definition.process.as_ref()?;
        self.lookups
            .recipes
            .get(&stack.item)?
            .iter()
            .map(|&i| &process.recipes[i])
            .find(|r| component_matches(&r.input_components, stack))
    }
    fn fuel(&self, stack: &Stack) -> Option<u16> {
        let process = self.definition.process.as_ref()?;
        self.lookups
            .fuels
            .get(&stack.item)?
            .iter()
            .map(|&i| &process.fuels[i])
            .find(|f| component_matches(&f.components, stack))
            .map(|f| f.pulses)
    }
    fn accepts_slot(&self, slot: usize, s: &Stack) -> bool {
        s.valid_in(&self.catalog) && self.accepts_kind(slot, s.item, s.components.is_some())
    }
    fn accepts_kind(&self, slot: usize, item: crate::items::ItemId, components: bool) -> bool {
        let Some(filter) = self.definition.filters.get(slot) else {
            return false;
        };
        (filter.components || !components)
            && (filter.items.is_empty() || self.lookups.filters[slot].contains(&item))
    }
    fn reset_input(&self, p: &mut MachinePayload, slot: usize, withdrawing: bool) {
        // Progress is attached to the exact input identity without extending the
        // save format: deposits can only merge identical components; every input
        // withdrawal resets progress before a replacement variant can arrive.
        // Catalog predicates are disjoint and frozen, so recovery resolves the
        // same recipe from the persisted input stack and progress_item.
        if let Some(process) = &self.definition.process
            && usize::from(process.input) == slot
        {
            let item = p.slots[slot].as_ref().map(|s| s.item);
            if withdrawing || p.progress_item != item {
                p.progress = 0;
            }
            p.progress_item = item;
        }
    }
}
fn component_matches(predicate: &api::ComponentMatch, stack: &Stack) -> bool {
    match predicate {
        api::ComponentMatch::Empty => stack.components.is_none(),
        api::ComponentMatch::Present => stack.components.is_some(),
        api::ComponentMatch::Exact(v) => stack
            .components
            .as_ref()
            .is_some_and(|c| c.version == v.version && c.bytes.as_ref() == v.bytes),
    }
}
pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: Arc<Catalog>,
    id: EntityTypeId,
) -> Result<(), EntityError> {
    let definition = catalog
        .machine(id)
        .cloned()
        .ok_or(EntityError::InvalidType)?;
    let anchors = definition
        .variants
        .iter()
        .flat_map(|v| v.idle.iter().chain(&v.active))
        .filter(|c| c.offset == [0; 3])
        .map(|c| {
            catalog
                .state_by_key(&c.state)
                .ok_or(EntityError::InvalidType)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let adapter = Arc::new(Adapter::new(catalog, definition.clone()));
    builder.register(EntityTypeRegistration {
        id,
        ownership: EntityOwnership::anchored(anchors, definition.variants[0].idle.len()),
        tick_policy: TickPolicy::Interval(definition.interval),
        max_payload_bytes: crate::inventory::container::max_bytes(definition.slots as usize) + 1040,
        codec: adapter.clone(),
    })?;
    builder.register_interaction_policy(id, adapter.clone())?;
    builder.register_transfer_policy(id, adapter.clone())?;
    builder.register_tick_planner(id, adapter)
}
