//! Deterministic kiln interaction, scheduled work, and break planning.

use super::model::{
    FUEL_SLOT_INDEX, INPUT_SLOT_INDEX, KILN_TICK_INTERVAL, KilnPayload, KilnRecipeBook, KilnSlot,
    OUTPUT_SLOT_INDEX, fuel_ticks,
};
use crate::content::{Catalog, KILN_ITEM};
use crate::inventory::{Inventory, SLOTS, STACK_LIMIT, Stack};
use crate::server::entities::transfer::{movable_count, put, take};
use crate::server::entities::{
    CellCoord, EntityBlockStateChange, EntityError, EntityInteractionPlan, EntityInteractionPolicy,
    EntityLocation, EntitySnapshot, EntityTickPlan, EntityTickPolicy, EntityView,
};
use crate::server::voxel_view::VoxelView;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnInsertPlan {
    pub payload: KilnPayload,
    pub remainder: Option<Stack>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnTakePlan {
    pub payload: KilnPayload,
    pub taken: Stack,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnBreakPlan {
    pub anchor: CellCoord,
    pub removed_cells: Vec<CellCoord>,
    pub drops: Vec<Stack>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnTickPlan {
    pub payload: Option<KilnPayload>,
    pub next_tick: u64,
}

pub(super) struct KilnInteractionPolicy;

pub(super) struct KilnTickPlanner {
    pub(super) recipes: Arc<KilnRecipeBook>,
}

impl EntityInteractionPolicy for KilnInteractionPolicy {
    fn reads_neighbours(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        catalog: &Catalog,
        _view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        let payload = snapshot
            .private_payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let request = if request.len() == 22 && request[0] == 2 {
            let id = u64::from_le_bytes(request[6..14].try_into().unwrap());
            if id != snapshot.id.get() {
                return Err(EntityError::InvalidPayload);
            }
            &request[..6]
        } else if request.len() == 6 && request[0] == 1 {
            request
        } else {
            return Err(EntityError::InvalidPayload);
        };
        let mut next_inventory = inventory.clone();
        let next_payload =
            plan_interaction_payload(payload, request, &mut next_inventory, catalog)?;
        let block_states = block_state_changes(snapshot, payload, &next_payload, catalog)?;
        Ok(EntityInteractionPlan {
            payload: next_payload.into_entity_payload(),
            inventory: next_inventory,
            block_states,
            wakes: Vec::new(),
        })
    }
}

impl EntityTickPolicy for KilnTickPlanner {
    fn reads_neighbours(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        current_tick: u64,
        catalog: &Catalog,
        _view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let payload = snapshot
            .private_payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let Some(due_tick) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due_tick {
            return Err(EntityError::InvalidType);
        }
        // Delayed admission catches up from the persisted due time instead of
        // silently sliding the schedule to whichever tick reached the queue.
        let tick = plan_tick(payload, &self.recipes, catalog, due_tick)?;
        let planned_payload = tick
            .payload
            .as_ref()
            .cloned()
            .unwrap_or_else(|| payload.clone());
        let anchor_update = planned_payload.anchor_update(
            snapshot.anchor().ok_or(EntityError::WrongOwnership)?,
            catalog,
        )?;
        let block_states = block_state_changes(snapshot, payload, &planned_payload, catalog)?;
        Ok(EntityTickPlan {
            lifecycle: Default::default(),
            payload: tick.payload.map(|payload| payload.into_entity_payload()),
            next_tick: Some(tick.next_tick),
            anchor_update: Some(anchor_update),
            position: None,
            block_states,
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

fn plan_interaction_payload(
    payload: &KilnPayload,
    request: &[u8],
    inventory: &mut Inventory,
    catalog: &Catalog,
) -> Result<KilnPayload, EntityError> {
    if request.len() != 6 || !matches!(request[0], 1 | 2) {
        return Err(EntityError::InvalidPayload);
    }
    let operation = request[1];
    let kiln_slot = KilnSlot::decode(request[2]).ok_or(EntityError::InvalidPayload)?;
    let inventory_slot = usize::from(request[3]);
    let count = u16::from_le_bytes([request[4], request[5]]);
    if inventory_slot >= SLOTS || count == 0 || count > STACK_LIMIT {
        return Err(EntityError::InvalidPayload);
    }
    match operation {
        0 if kiln_slot != KilnSlot::Output => {
            let amount = movable_count(
                &inventory.slots[inventory_slot],
                &payload.slots[kiln_slot.index()],
                count,
            )
            .ok_or(EntityError::InvalidPayload)?;
            let source = inventory.slots[inventory_slot]
                .as_ref()
                .ok_or(EntityError::InvalidPayload)?;
            let mut incoming = source.clone();
            incoming.count = amount;
            let planned = plan_insert(payload, kiln_slot, &incoming, catalog)?;
            if planned.remainder.is_some() {
                return Err(EntityError::InvalidPayload);
            }
            take(&mut inventory.slots[inventory_slot], amount)
                .ok_or(EntityError::InvalidPayload)?;
            bump_inventory_revision(inventory)?;
            Ok(planned.payload)
        }
        1 => {
            let amount = movable_count(
                &payload.slots[kiln_slot.index()],
                &inventory.slots[inventory_slot],
                count,
            )
            .ok_or(EntityError::InvalidPayload)?;
            let planned = plan_take(payload, kiln_slot, amount, catalog)?;
            let destination = &mut inventory.slots[inventory_slot];
            if !put(destination, &planned.taken) {
                return Err(EntityError::InvalidPayload);
            }
            bump_inventory_revision(inventory)?;
            Ok(planned.payload)
        }
        _ => Err(EntityError::InvalidPayload),
    }
}

fn bump_inventory_revision(inventory: &mut Inventory) -> Result<(), EntityError> {
    inventory.revision = inventory
        .revision
        .checked_add(1)
        .ok_or(EntityError::RevisionExhausted)?;
    Ok(())
}

fn block_state_changes(
    snapshot: &EntitySnapshot,
    before: &KilnPayload,
    after: &KilnPayload,
    catalog: &Catalog,
) -> Result<Vec<EntityBlockStateChange>, EntityError> {
    let (anchor, footprint) = match &snapshot.location {
        EntityLocation::Anchored {
            anchor, footprint, ..
        } => (*anchor, footprint),
        EntityLocation::Mobile { .. } => return Err(EntityError::WrongOwnership),
    };
    let canonical_footprint = super::super::kiln_footprint(anchor)?;
    if footprint != &canonical_footprint {
        return Err(EntityError::InvalidLocation);
    }
    let before_states = super::super::kiln_block_states(catalog, before)?;
    let after_states = super::super::kiln_block_states(catalog, after)?;
    let upper = CellCoord::new(
        anchor.x,
        anchor
            .y
            .checked_add(1)
            .ok_or(EntityError::InvalidLocation)?,
        anchor.z,
    );
    let mut changes = vec![
        EntityBlockStateChange {
            cell: anchor,
            before: before_states[0],
            after: after_states[0],
        },
        EntityBlockStateChange {
            cell: upper,
            before: before_states[1],
            after: after_states[1],
        },
    ];
    changes.sort_by_key(|change| change.cell);
    Ok(changes)
}

/// Transfer exact stack remainders into one kiln inventory slot. Output is
/// never insertable; fuel accepts only catalogued fuel items.
pub(in crate::server) fn plan_insert(
    payload: &KilnPayload,
    slot: KilnSlot,
    incoming: &Stack,
    catalog: &Catalog,
) -> Result<KilnInsertPlan, EntityError> {
    payload.validate(catalog)?;
    if slot == KilnSlot::Output || !incoming.valid_in(catalog) {
        return Err(EntityError::InvalidPayload);
    }
    if slot == KilnSlot::Fuel && fuel_ticks(incoming, catalog).is_none() {
        return Err(EntityError::InvalidPayload);
    }
    let mut next = payload.clone();
    let index = slot.index();
    let mut remaining = incoming.count;
    match &mut next.slots[index] {
        Some(current)
            if current.item == incoming.item && current.components == incoming.components =>
        {
            let added = remaining.min(STACK_LIMIT - current.count);
            current.count += added;
            remaining -= added;
        }
        Some(_) => {}
        empty @ None => {
            let added = remaining.min(STACK_LIMIT);
            let mut stack = incoming.clone();
            stack.count = added;
            *empty = Some(stack);
            remaining -= added;
        }
    }
    if slot == KilnSlot::Input && next.slots[INPUT_SLOT_INDEX].is_some() {
        let input = next.slots[INPUT_SLOT_INDEX].as_ref().unwrap();
        next.progress_item = input.components.is_none().then_some(input.item);
        if next.progress_item != payload.progress_item {
            next.cook_progress = 0;
        }
    }
    next.validate(catalog)?;
    let remainder = if remaining == 0 {
        None
    } else {
        let mut stack = incoming.clone();
        stack.count = remaining;
        Some(stack)
    };
    Ok(KilnInsertPlan {
        payload: next,
        remainder,
    })
}

/// Transfer an exact count out of a kiln slot. Removing the current input
/// resets its partial cook progress; already-started fuel is not refunded.
pub(in crate::server) fn plan_take(
    payload: &KilnPayload,
    slot: KilnSlot,
    count: u16,
    catalog: &Catalog,
) -> Result<KilnTakePlan, EntityError> {
    payload.validate(catalog)?;
    let mut next = payload.clone();
    let stack = next.slots[slot.index()]
        .as_mut()
        .ok_or(EntityError::InvalidPayload)?;
    if count == 0 || count > stack.count {
        return Err(EntityError::InvalidPayload);
    }
    let mut taken = stack.clone();
    taken.count = count;
    stack.count -= count;
    if stack.count == 0 {
        next.slots[slot.index()] = None;
    }
    if slot == KilnSlot::Input {
        next.cook_progress = 0;
        next.progress_item = next.slots[INPUT_SLOT_INDEX]
            .as_ref()
            .filter(|input| input.components.is_none())
            .map(|input| input.item);
    }
    next.validate(catalog)?;
    Ok(KilnTakePlan {
        payload: next,
        taken,
    })
}

/// Plan one scheduled kiln pulse. A due-time step is persisted even if no
/// work can happen. Existing fuel burns one unit per pulse while idle or
/// output-blocked, but new fuel starts only for a valid recipe that fits.
pub(in crate::server) fn plan_tick(
    payload: &KilnPayload,
    recipes: &KilnRecipeBook,
    catalog: &Catalog,
    current_tick: u64,
) -> Result<KilnTickPlan, EntityError> {
    payload.validate(catalog)?;
    let next_tick = current_tick
        .checked_add(KILN_TICK_INTERVAL)
        .ok_or(EntityError::RevisionExhausted)?;
    let mut next = payload.clone();
    let Some(input) = next.slots[INPUT_SLOT_INDEX].as_ref() else {
        next.cook_progress = 0;
        next.progress_item = None;
        burn_one_step(&mut next);
        next.validate(catalog)?;
        return Ok(KilnTickPlan {
            payload: (next != *payload).then_some(next),
            next_tick,
        });
    };
    let Some(recipe) = recipes.recipe(input).cloned() else {
        next.cook_progress = 0;
        next.progress_item = None;
        burn_one_step(&mut next);
        next.validate(catalog)?;
        return Ok(KilnTickPlan {
            payload: (next != *payload).then_some(next),
            next_tick,
        });
    };
    if next.progress_item != Some(input.item) {
        next.progress_item = Some(input.item);
        next.cook_progress = 0;
    }
    if !output_fits(next.slots[OUTPUT_SLOT_INDEX].as_ref(), &recipe.output) {
        burn_one_step(&mut next);
        next.validate(catalog)?;
        return Ok(KilnTickPlan {
            payload: (next != *payload).then_some(next),
            next_tick,
        });
    }
    if next.fuel_remaining == 0 {
        let Some(fuel) = next.slots[FUEL_SLOT_INDEX].as_mut() else {
            return Ok(KilnTickPlan {
                payload: (next != *payload).then_some(next),
                next_tick,
            });
        };
        let Some(burn_ticks) = fuel_ticks(fuel, catalog) else {
            return Err(EntityError::InvalidPayload);
        };
        if fuel.count == 0 {
            return Err(EntityError::InvalidPayload);
        }
        fuel.count -= 1;
        if fuel.count == 0 {
            next.slots[FUEL_SLOT_INDEX] = None;
        }
        next.fuel_remaining = burn_ticks;
        next.lit = true;
    }
    burn_one_step(&mut next);
    next.cook_progress = next.cook_progress.saturating_add(1);
    if next.cook_progress >= recipe.cook_ticks {
        let input = next.slots[INPUT_SLOT_INDEX]
            .as_mut()
            .ok_or(EntityError::InvalidPayload)?;
        input.count -= 1;
        if input.count == 0 {
            next.slots[INPUT_SLOT_INDEX] = None;
            next.progress_item = None;
        }
        insert_output(&mut next.slots[OUTPUT_SLOT_INDEX], &recipe.output)?;
        next.cook_progress = 0;
    }
    next.validate(catalog)?;
    Ok(KilnTickPlan {
        payload: (next != *payload).then_some(next),
        next_tick,
    })
}

/// Either half resolves to the lower anchor and removes the complete
/// footprint. The exact private inventory stacks are returned once.
pub(in crate::server) fn plan_break(
    anchor: CellCoord,
    broken_cell: CellCoord,
    payload: &KilnPayload,
    catalog: &Catalog,
) -> Result<KilnBreakPlan, EntityError> {
    payload.validate(catalog)?;
    let removed_cells = super::kiln_footprint(anchor)?;
    if removed_cells.binary_search(&broken_cell).is_err() {
        return Err(EntityError::InvalidLocation);
    }
    let mut drops = vec![Stack::new(KILN_ITEM, 1)];
    drops.extend(payload.slots.iter().flatten().cloned());
    Ok(KilnBreakPlan {
        anchor,
        removed_cells,
        drops,
    })
}

fn burn_one_step(payload: &mut KilnPayload) {
    payload.fuel_remaining = payload.fuel_remaining.saturating_sub(1);
    payload.lit = payload.fuel_remaining > 0;
}

fn output_fits(current: Option<&Stack>, incoming: &Stack) -> bool {
    match current {
        None => true,
        Some(current)
            if current.item == incoming.item && current.components == incoming.components =>
        {
            u32::from(current.count) + u32::from(incoming.count) <= u32::from(STACK_LIMIT)
        }
        Some(_) => false,
    }
}

fn insert_output(slot: &mut Option<Stack>, incoming: &Stack) -> Result<(), EntityError> {
    match slot {
        None => *slot = Some(incoming.clone()),
        Some(current)
            if current.item == incoming.item && current.components == incoming.components =>
        {
            if u32::from(current.count) + u32::from(incoming.count) > u32::from(STACK_LIMIT) {
                return Err(EntityError::InvalidPayload);
            }
            current.count += incoming.count;
        }
        Some(_) => return Err(EntityError::InvalidPayload),
    }
    Ok(())
}
