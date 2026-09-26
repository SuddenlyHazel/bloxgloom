//! Kiln's validated private state model and recipe declarations.

use super::{kiln_footprint, kiln_state};
use crate::content::{BlockStateId, Catalog, KILN_BLOCK_TYPE, KILN_ENTITY_TYPE};
use crate::inventory::Stack;
use crate::items::{ItemId, SAPLING, STICK};
use crate::server::entities::EntitySpawn;
use crate::server::entities::registry::EntityCodecError;
use crate::server::entities::types::{AnchorUpdate, EntityError, EntityPayload};
use crate::world;

pub(super) const KILN_MAX_PAYLOAD_BYTES: usize = 3_200;
pub(super) const KILN_MAX_COOK_TICKS: u16 = 60_000;
pub(super) const KILN_MAX_FUEL_TICKS: u16 = 240;
pub(super) const KILN_TICK_INTERVAL: u64 = 20;
const KILN_MAX_RECIPES: usize = 4_096;
pub(super) const FUEL_SLOT_INDEX: usize = 0;
pub(super) const INPUT_SLOT_INDEX: usize = 1;
pub(super) const OUTPUT_SLOT_INDEX: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum KilnFacing {
    North,
    East,
    South,
    West,
}

impl KilnFacing {
    pub(super) const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub(super) const fn encoded(self) -> u8 {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }

    #[cfg(test)]
    pub(in crate::server) fn from_player_yaw(yaw: f32) -> Result<Self, EntityError> {
        if !yaw.is_finite() {
            return Err(EntityError::InvalidLocation);
        }
        let forward_x = yaw.cos();
        let forward_z = yaw.sin();
        Ok(if forward_x.abs() >= forward_z.abs() {
            if forward_x > 0.0 {
                Self::West
            } else {
                Self::East
            }
        } else if forward_z > 0.0 {
            Self::North
        } else {
            Self::South
        })
    }

    /// Read a trusted placement orientation hint. Only an unlit lower kiln
    /// state is accepted; the server later derives both canonical halves.
    pub(in crate::server) fn from_place_state(
        catalog: &Catalog,
        state_id: BlockStateId,
    ) -> Result<Self, EntityError> {
        let state = catalog.state(state_id).ok_or(EntityError::InvalidType)?;
        if state.block_type != KILN_BLOCK_TYPE
            || state.properties.iter().any(|(name, value)| {
                (name == "half" && value != "lower") || (name == "lit" && value != "false")
            })
        {
            return Err(EntityError::InvalidType);
        }
        let facing = state
            .properties
            .iter()
            .find(|(name, _)| name == "facing")
            .map(|(_, value)| value.as_str());
        match facing {
            Some("north") => Ok(Self::North),
            Some("east") => Ok(Self::East),
            Some("south") => Ok(Self::South),
            Some("west") => Ok(Self::West),
            _ => Err(EntityError::InvalidType),
        }
    }

    pub(super) fn decode(value: u8) -> Result<Self, EntityCodecError> {
        match value {
            0 => Ok(Self::North),
            1 => Ok(Self::East),
            2 => Ok(Self::South),
            3 => Ok(Self::West),
            _ => Err(EntityCodecError::InvalidData),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum KilnHalf {
    Lower,
    Upper,
}

impl KilnHalf {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }
}

/// Private WAL-owned inventory/progress. Public projections are assembled by
/// the codec; public slot summaries omit arbitrary item component payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnPayload {
    pub(super) facing: KilnFacing,
    pub(super) lit: bool,
    pub(super) fuel_remaining: u16,
    pub(super) cook_progress: u16,
    pub(super) progress_item: Option<ItemId>,
    pub(super) slots: [Option<Stack>; 3],
}

impl KilnPayload {
    pub(in crate::server) fn new(facing: KilnFacing) -> Self {
        Self {
            facing,
            lit: false,
            fuel_remaining: 0,
            cook_progress: 0,
            progress_item: None,
            slots: std::array::from_fn(|_| None),
        }
    }

    #[cfg(test)]
    pub(in crate::server) const fn is_lit(&self) -> bool {
        self.lit
    }

    #[cfg(test)]
    pub(in crate::server) const fn fuel_remaining(&self) -> u16 {
        self.fuel_remaining
    }

    #[cfg(test)]
    pub(in crate::server) const fn cook_progress(&self) -> u16 {
        self.cook_progress
    }

    #[cfg(test)]
    pub(in crate::server) fn slot(&self, slot: KilnSlot) -> Option<&Stack> {
        self.slots.get(slot.index()).and_then(Option::as_ref)
    }

    pub(in crate::server) fn into_entity_payload(self) -> EntityPayload {
        EntityPayload::new(self)
    }

    pub(in crate::server) fn anchor_update(
        &self,
        anchor: crate::server::entities::CellCoord,
        catalog: &Catalog,
    ) -> Result<AnchorUpdate, EntityError> {
        Ok(AnchorUpdate {
            anchor,
            anchor_state: kiln_state(catalog, KilnHalf::Lower, self.facing, self.lit)?,
            footprint: kiln_footprint(anchor)?,
        })
    }

    pub(in crate::server) fn spawn(
        self,
        anchor: crate::server::entities::CellCoord,
        spawn_tick: u64,
        catalog: &Catalog,
    ) -> Result<EntitySpawn, EntityError> {
        self.validate(catalog)?;
        let update = self.anchor_update(anchor, catalog)?;
        Ok(EntitySpawn::Anchored {
            entity_type: KILN_ENTITY_TYPE,
            anchor: update.anchor,
            anchor_state: update.anchor_state,
            footprint: update.footprint,
            payload: self.into_entity_payload(),
            spawn_tick,
        })
    }

    pub(super) fn validate(&self, catalog: &Catalog) -> Result<(), EntityError> {
        if self.lit != (self.fuel_remaining > 0)
            || self.fuel_remaining > KILN_MAX_FUEL_TICKS
            || self.cook_progress > KILN_MAX_COOK_TICKS
        {
            return Err(EntityError::InvalidPayload);
        }
        let input_item = self.slots[INPUT_SLOT_INDEX]
            .as_ref()
            .filter(|stack| stack.components.is_none())
            .map(|stack| stack.item);
        let valid_progress = if self.cook_progress == 0 {
            self.progress_item
                .is_none_or(|progress_item| Some(progress_item) == input_item)
        } else {
            input_item.is_some() && self.progress_item == input_item
        };
        if !valid_progress {
            return Err(EntityError::InvalidPayload);
        }
        for stack in self.slots.iter().flatten() {
            if !stack.valid_in(catalog) {
                return Err(EntityError::InvalidPayload);
            }
        }
        if self.slots[FUEL_SLOT_INDEX]
            .as_ref()
            .is_some_and(|stack| fuel_ticks(stack, catalog).is_none())
        {
            return Err(EntityError::InvalidPayload);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum KilnSlot {
    Fuel,
    Input,
    Output,
}

impl KilnSlot {
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Fuel => FUEL_SLOT_INDEX,
            Self::Input => INPUT_SLOT_INDEX,
            Self::Output => OUTPUT_SLOT_INDEX,
        }
    }

    pub(super) const fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Fuel),
            1 => Some(Self::Input),
            2 => Some(Self::Output),
            _ => None,
        }
    }
}

/// One-input immutable recipe; `cook_ticks` counts scheduled kiln pulses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct KilnRecipe {
    pub input: ItemId,
    pub output: Stack,
    pub cook_ticks: u16,
}

#[derive(Clone, Debug, Default)]
pub(in crate::server) struct KilnRecipeBook {
    recipes: std::collections::BTreeMap<ItemId, KilnRecipe>,
}

impl KilnRecipeBook {
    pub(in crate::server) fn builtins(catalog: &Catalog) -> Result<Self, EntityError> {
        Self::new(
            [KilnRecipe {
                input: ItemId(world::GRAVEL.0),
                output: Stack::new(ItemId(world::STONE.0), 1),
                cook_ticks: 4,
            }],
            catalog,
        )
    }

    pub(in crate::server) fn new(
        recipes: impl IntoIterator<Item = KilnRecipe>,
        catalog: &Catalog,
    ) -> Result<Self, EntityError> {
        let mut book = Self::default();
        for recipe in recipes {
            if book.recipes.len() >= KILN_MAX_RECIPES
                || catalog.item(recipe.input).is_none()
                || !recipe.output.valid_in(catalog)
                || recipe.cook_ticks == 0
                || recipe.cook_ticks > KILN_MAX_COOK_TICKS
                || book.recipes.contains_key(&recipe.input)
            {
                return Err(EntityError::InvalidType);
            }
            book.recipes.insert(recipe.input, recipe);
        }
        Ok(book)
    }

    pub(super) fn recipe(&self, input: &Stack) -> Option<&KilnRecipe> {
        if input.components.is_some() {
            None
        } else {
            self.recipes.get(&input.item)
        }
    }
}

pub(super) fn fuel_ticks(stack: &Stack, catalog: &Catalog) -> Option<u16> {
    if stack.components.is_some() || catalog.item(stack.item).is_none() {
        return None;
    }
    if stack.item == STICK {
        return Some(40);
    }
    if stack.item == SAPLING {
        return Some(80);
    }
    let placeable = catalog.item(stack.item)?.placeable?;
    let state = catalog.state(placeable)?;
    let block = catalog.block_type(state.block_type)?;
    if !block.flammable {
        return None;
    }
    Some(if state.block_type.0 == world::WOOD.0 {
        240
    } else {
        60
    })
}
