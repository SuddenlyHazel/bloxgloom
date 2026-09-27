use super::super::transfer::{AutomationStack, put, take};
use super::*;
use crate::items::ItemId;
#[cfg(test)]
use crate::protocol::workstation::WorkstationView;
use crate::server::voxel_view::VoxelView;

#[cfg(test)]
fn public_slots(public: &[u8]) -> Option<Vec<Option<Stack>>> {
    WorkstationView::decode(public).map(|view| view.slots)
}

pub(in crate::server::entities) struct Port<P> {
    screen: Option<Arc<bloxgloom_host_api::InventoryScreen>>,
    slot: Option<u8>,
    payload: std::marker::PhantomData<P>,
}
impl<P> Port<P> {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            screen: None,
            slot: None,
            payload: std::marker::PhantomData,
        }
    }
    pub fn for_screen(screen: Arc<bloxgloom_host_api::InventoryScreen>) -> Self {
        Self {
            screen: Some(screen),
            slot: None,
            payload: std::marker::PhantomData,
        }
    }
    fn permits(&self, slot: usize, insert: bool) -> bool {
        self.slot.is_none_or(|wanted| usize::from(wanted) == slot)
            && self.screen.as_ref().is_none_or(|s| {
                s.group(slot as u8)
                    .is_some_and(|g| if insert { g.insert } else { g.extract })
            })
    }
}
impl<P: Slots> EntityTransferPolicy for Port<P> {
    fn inventory_accepts(&self, slot: u8, stack: &Stack, catalog: &Catalog) -> bool {
        self.permits(usize::from(slot), true) && stack.valid_in(catalog)
    }
    fn replace_inventory(
        &self,
        payload: &EntityPayload,
        slots: Vec<Option<Stack>>,
        catalog: &Catalog,
    ) -> Result<EntityPayload, EntityError> {
        let mut payload = payload
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        let destination = payload.slots_mut();
        if destination.len() != slots.len()
            || slots.iter().flatten().any(|stack| !stack.valid_in(catalog))
        {
            return Err(EntityError::InvalidPayload);
        }
        destination.clone_from_slice(&slots);
        Ok(EntityPayload::new(payload))
    }
    fn at_slot(&self, slot: u8) -> Option<Arc<dyn EntityTransferPolicy>> {
        (slot < 54 && self.slot.is_none_or(|old| old == slot)).then(|| {
            Arc::new(Self {
                screen: self.screen.clone(),
                slot: Some(slot),
                payload: std::marker::PhantomData,
            }) as Arc<dyn EntityTransferPolicy>
        })
    }
    fn automation_offers(&self, slots: &[Option<AutomationStack>]) -> Vec<(u8, AutomationStack)> {
        slots
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| self.permits(*i, false))
            .filter_map(|(i, s)| Some((i as u8, s?)))
            .collect()
    }
    fn automation_accepts(
        &self,
        slots: &[Option<AutomationStack>],
        stack: AutomationStack,
    ) -> bool {
        slots
            .iter()
            .enumerate()
            .any(|(i, slot)| self.permits(i, true) && stack.fits(*slot))
    }
    fn inventory_slots(&self, payload: &EntityPayload) -> Result<Vec<Option<Stack>>, EntityError> {
        Ok(payload
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone()
            .slots_mut()
            .to_vec())
    }
    fn ports(&self) -> Vec<String> {
        vec!["storage".into()]
    }
    fn port(&self, index: u8, face: [i32; 3]) -> Option<Arc<dyn EntityTransferPolicy>> {
        (index == 0 && bloxgloom_host_api::machine::FACES.contains(&face)).then(|| {
            Arc::new(Self {
                screen: self.screen.clone(),
                slot: self.slot,
                payload: std::marker::PhantomData,
            }) as Arc<dyn EntityTransferPolicy>
        })
    }
    #[cfg(test)]
    fn offers(&self, public: &[u8]) -> Vec<Stack> {
        public_slots(public).map_or_else(Vec::new, |slots| {
            slots
                .into_iter()
                .enumerate()
                .filter(|(i, _)| self.permits(*i, false))
                .filter_map(|(_, s)| s)
                .collect()
        })
    }
    #[cfg(test)]
    fn accepts(&self, public: &[u8], stack: &Stack, _: &Catalog) -> bool {
        public_slots(public).is_some_and(|mut slots| {
            slots
                .iter_mut()
                .enumerate()
                .any(|(index, slot)| self.permits(index, true) && put(slot, stack))
        })
    }
    fn withdraw(
        &self,
        payload: &EntityPayload,
        item: ItemId,
        count: u16,
        _: &Catalog,
    ) -> Result<Option<(EntityPayload, Stack)>, EntityError> {
        let mut payload = payload
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        for (index, slot) in payload.slots_mut().iter_mut().enumerate() {
            if self.permits(index, false)
                && slot
                    .as_ref()
                    .is_some_and(|s| s.item == item && s.count >= count)
            {
                let stack = take(slot, count).ok_or(EntityError::InvalidPayload)?;
                return Ok(Some((EntityPayload::new(payload), stack)));
            }
        }
        Ok(None)
    }
    fn deposit(
        &self,
        payload: &EntityPayload,
        stack: &Stack,
        catalog: &Catalog,
    ) -> Result<Option<EntityPayload>, EntityError> {
        if !stack.valid_in(catalog) {
            return Err(EntityError::InvalidPayload);
        }
        let mut payload = payload
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        if payload
            .slots_mut()
            .iter_mut()
            .enumerate()
            .any(|(index, slot)| self.permits(index, true) && put(slot, stack))
        {
            Ok(Some(EntityPayload::new(payload)))
        } else {
            Ok(None)
        }
    }
}

pub(in crate::server::entities) struct Interaction<P>(std::marker::PhantomData<P>);
impl<P> Interaction<P> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}
fn block(
    snapshot: &EntitySnapshot,
    view: &VoxelView,
) -> Result<Vec<EntityBlockStateChange>, EntityError> {
    let EntityLocation::Anchored { ref footprint, .. } = snapshot.location else {
        return Err(EntityError::WrongOwnership);
    };
    footprint
        .iter()
        .map(|&cell| {
            let state = view
                .block(cell.x, cell.y, cell.z)
                .map_err(|_| EntityError::InvalidPayload)?;
            Ok(EntityBlockStateChange {
                cell,
                before: state,
                after: state,
            })
        })
        .collect()
}
impl<P: Slots> EntityInteractionPolicy for Interaction<P> {
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        catalog: &Catalog,
        view: &VoxelView,
        _: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        if request.len() != 22
            || request[0] != 2
            || request[1] > 1
            || usize::from(request[3]) >= crate::inventory::SLOTS
            || u64::from_le_bytes(request[6..14].try_into().unwrap()) != snapshot.id.get()
            || u64::from_le_bytes(request[14..22].try_into().unwrap()) != snapshot.revision
        {
            return Err(EntityError::InvalidPayload);
        }
        let count = u16::from_le_bytes([request[4], request[5]]);
        let group = catalog
            .inventory_screen(snapshot.entity_type)
            .and_then(|screen| screen.group(request[2]))
            .ok_or(EntityError::InvalidPayload)?;
        if (request[1] == 0 && !group.insert) || (request[1] == 1 && !group.extract) {
            return Err(EntityError::InvalidPayload);
        }
        if count == 0 || count > STACK_LIMIT {
            return Err(EntityError::InvalidPayload);
        }
        let mut payload = snapshot
            .private_payload
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        let mut inventory = inventory.clone();
        let machine = payload
            .slots_mut()
            .get_mut(request[2] as usize)
            .ok_or(EntityError::InvalidPayload)?;
        let player = &mut inventory.slots[request[3] as usize];
        let (source, destination) = if request[1] == 0 {
            (player, machine)
        } else {
            (machine, player)
        };
        let stack = take(source, count).ok_or(EntityError::InvalidPayload)?;
        if !put(destination, &stack) {
            return Err(EntityError::InvalidPayload);
        }
        inventory.revision = inventory
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        Ok(EntityInteractionPlan {
            payload: EntityPayload::new(payload),
            inventory,
            block_states: block(snapshot, view)?,
            wakes: vec![],
        })
    }
}
