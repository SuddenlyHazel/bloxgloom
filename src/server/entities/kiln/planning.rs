//! Deterministic kiln interaction, scheduled work, and break planning.

use super::model::{
    FUEL_SLOT_INDEX, INPUT_SLOT_INDEX, KILN_TICK_INTERVAL, KilnPayload, KilnRecipeBook, KilnSlot,
    OUTPUT_SLOT_INDEX, fuel_ticks,
};
use crate::content::{Catalog, KILN_ITEM};
use crate::inventory::{STACK_LIMIT, Stack};
use crate::server::entities::{CellCoord, EntityError, EntityPatch};

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

impl KilnTickPlan {
    pub(in crate::server) fn entity_patch(self) -> EntityPatch {
        EntityPatch {
            payload: self.payload.map(KilnPayload::into_entity_payload),
            next_tick: Some(Some(self.next_tick)),
            position: None,
        }
    }
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
