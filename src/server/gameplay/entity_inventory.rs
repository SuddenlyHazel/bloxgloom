use super::super::durable::TerrainReads;
use super::super::entities::{EntityId, EntityPatch, EntityStore, PreparedEntityTransaction};
use crate::content::Catalog;
use bloxgloom_host_api::gameplay::{Error, InventoryId, Slot, Stack};

pub(super) fn capture(
    catalog: &Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    raw: u64,
) -> Result<Vec<Slot>, Error> {
    let unavailable = || Error::InventoryUnavailable(InventoryId::Entity(raw));
    let id = EntityId::new(raw).ok_or_else(unavailable)?;
    reads
        .entities(store.capture_entity_dependency(id))
        .map_err(|_| Error::BudgetExceeded)?;
    let snapshot = store.snapshot(id).ok_or_else(unavailable)?;
    if snapshot.entity_type == crate::server::drops::DROP_ENTITY_TYPE {
        let stack = crate::server::drops::stack(store, id).ok_or_else(unavailable)?;
        let mut slots = super::inventory::slots(catalog, &[Some(stack)])?;
        slots[0].insert = false;
        slots[0].extract = crate::server::drops::extractable(store, id);
        return Ok(slots);
    }
    let descriptor = store
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(|_| unavailable())?;
    let policy = descriptor.transfer_policy().ok_or_else(unavailable)?;
    let screen = catalog
        .inventory_screen(snapshot.entity_type)
        .ok_or_else(unavailable)?;
    let slots = policy
        .inventory_slots(&snapshot.private_payload)
        .map_err(|e| Error::Host(e.to_string()))?;
    let mut slots = super::inventory::slots(catalog, &slots)?;
    for (i, slot) in slots.iter_mut().enumerate() {
        let group = screen.group(i as u8);
        slot.insert = group.is_some_and(|g| g.insert);
        slot.extract = group.is_some_and(|g| g.extract);
    }
    Ok(slots)
}

pub(super) fn accepts(
    catalog: &Catalog,
    store: &EntityStore,
    raw: u64,
    slot: usize,
    stack: &Stack,
) -> bool {
    let Some(id) = EntityId::new(raw) else {
        return false;
    };
    let Some(snapshot) = store.snapshot(id) else {
        return false;
    };
    let Ok(descriptor) = store.types().descriptor(snapshot.entity_type) else {
        return false;
    };
    let Some(policy) = descriptor.transfer_policy() else {
        return false;
    };
    let Ok(slot) = u8::try_from(slot) else {
        return false;
    };
    let Ok(stack) = super::inventory::stack(catalog, stack) else {
        return false;
    };
    policy.inventory_accepts(slot, &stack, catalog)
}

pub(super) fn prepare(
    catalog: &Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    raw: u64,
    after: Vec<Option<Stack>>,
) -> Result<Option<PreparedEntityTransaction>, Error> {
    let before = capture(catalog, reads, store, raw)?;
    let id = EntityId::new(raw).ok_or(Error::InventoryUnavailable(InventoryId::Entity(raw)))?;
    if store
        .snapshot(id)
        .is_some_and(|snapshot| snapshot.entity_type == crate::server::drops::DROP_ENTITY_TYPE)
    {
        if after.len() != 1 {
            return Err(Error::Invalid("world drop has exactly one slot".into()));
        }
        let Some(original) = &before[0].stack else {
            return Err(Error::Host("drop stack vanished".into()));
        };
        let remaining = match &after[0] {
            Some(stack)
                if stack.item == original.item
                    && stack.components == original.components
                    && stack.count <= original.count =>
            {
                stack.count
            }
            None => 0,
            _ => {
                return Err(Error::Invalid(
                    "world drop cannot create or replace items".into(),
                ));
            }
        };
        let removed = original.count - remaining;
        if removed == 0 {
            return Ok(None);
        }
        return crate::server::drops::plan_take(store, &[(id, removed)])
            .map_err(|error| Error::Invalid(error.to_string()));
    }
    if before.len() != after.len() {
        return Err(Error::Invalid("entity inventory shape changed".into()));
    }
    let mut changed = false;
    for (slot, (before, after)) in before.iter().zip(&after).enumerate() {
        if before.stack == *after {
            continue;
        }
        changed = true;
        let same = before
            .stack
            .as_ref()
            .zip(after.as_ref())
            .is_some_and(|(a, b)| a.item == b.item && a.components == b.components);
        let removes = before
            .stack
            .as_ref()
            .is_some_and(|old| !same || after.as_ref().is_some_and(|new| new.count < old.count));
        let inserts = after.as_ref().is_some_and(|new| {
            !same
                || before
                    .stack
                    .as_ref()
                    .is_some_and(|old| new.count > old.count)
        });
        if (removes && !before.extract)
            || (inserts
                && (!before.insert || !accepts(catalog, store, raw, slot, after.as_ref().unwrap())))
        {
            return Err(Error::Invalid(
                "entity inventory slot permission or filter rejected change".into(),
            ));
        }
    }
    if !changed {
        return Ok(None);
    }
    let id = EntityId::new(raw).unwrap();
    let snapshot = store.snapshot(id).unwrap();
    let policy = store
        .types()
        .descriptor(snapshot.entity_type)
        .unwrap()
        .transfer_policy()
        .unwrap();
    let after = after
        .iter()
        .map(|stack| {
            stack
                .as_ref()
                .map(|stack| super::inventory::stack(catalog, stack))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let payload = policy
        .replace_inventory(&snapshot.private_payload, after, catalog)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    store
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: Some(payload),
                ..Default::default()
            },
        )
        .map(Some)
        .map_err(|e| Error::Invalid(e.to_string()))
}
