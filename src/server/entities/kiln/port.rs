//! Automated inventory capability: accept fuel/input, expose finished output.
use super::model::{KilnPayload, KilnRecipeBook, KilnSlot, fuel_ticks};
use super::planning::{plan_insert, plan_take};
use crate::content::Catalog;
use crate::inventory::Stack;
use crate::items::ItemId;
use crate::protocol::workstation::WorkstationView;
use crate::server::entities::{EntityError, EntityPayload, EntityTransferPolicy};

pub(super) struct Port {
    pub recipes: std::sync::Arc<KilnRecipeBook>,
}
impl EntityTransferPolicy for Port {
    fn offers(&self, public: &[u8]) -> Vec<Stack> {
        WorkstationView::decode(public)
            .and_then(|v| v.slots[2].clone())
            .into_iter()
            .collect()
    }
    fn accepts(&self, public: &[u8], stack: &Stack, catalog: &Catalog) -> bool {
        if fuel_ticks(stack, catalog).is_none() && self.recipes.recipe(stack).is_none() {
            return false;
        }
        WorkstationView::decode(public).is_some_and(|v| {
            let index = if fuel_ticks(stack, catalog).is_some() {
                0
            } else {
                1
            };
            let mut slot = v.slots[index].clone();
            crate::server::entities::transfer::put(&mut slot, stack)
        })
    }
    fn withdraw(
        &self,
        payload: &EntityPayload,
        item: ItemId,
        count: u16,
        catalog: &Catalog,
    ) -> Result<Option<(EntityPayload, Stack)>, EntityError> {
        let payload = payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        if !payload.slots[2]
            .as_ref()
            .is_some_and(|s| s.item == item && s.count >= count)
        {
            return Ok(None);
        }
        let planned = plan_take(payload, KilnSlot::Output, count, catalog)?;
        Ok(Some((planned.payload.into_entity_payload(), planned.taken)))
    }
    fn deposit(
        &self,
        payload: &EntityPayload,
        stack: &Stack,
        catalog: &Catalog,
    ) -> Result<Option<EntityPayload>, EntityError> {
        if fuel_ticks(stack, catalog).is_none() && self.recipes.recipe(stack).is_none() {
            return Ok(None);
        }
        let payload = payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let slot = if fuel_ticks(stack, catalog).is_some() {
            KilnSlot::Fuel
        } else {
            KilnSlot::Input
        };
        match plan_insert(payload, slot, stack, catalog) {
            Ok(plan) if plan.remainder.is_none() => Ok(Some(plan.payload.into_entity_payload())),
            _ => Ok(None),
        }
    }
}
