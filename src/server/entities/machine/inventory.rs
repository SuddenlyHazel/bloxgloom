use super::super::transfer::{put, take};
use super::*;
use crate::{inventory::Inventory, server::voxel_view::VoxelView};
impl Adapter {
    fn allowed(&self, insert: bool) -> Vec<u8> {
        self.port
            .and_then(|i| self.definition.ports.get(i as usize))
            .map_or_else(Vec::new, |p| {
                if insert {
                    p.insert.clone()
                } else {
                    p.extract.clone()
                }
            })
    }
}
impl EntityTransferPolicy for Adapter {
    fn ports(&self) -> Vec<String> {
        self.definition
            .ports
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }
    fn port(&self, index: u8, face: [i32; 3]) -> Option<Arc<dyn EntityTransferPolicy>> {
        self.definition
            .ports
            .get(index as usize)
            .filter(|p| p.faces.contains(&face))
            .map(|_| {
                Arc::new(Self {
                    port: Some(index),
                    ..self.clone()
                }) as Arc<dyn EntityTransferPolicy>
            })
    }
    fn offers(&self, public: &[u8]) -> Vec<Stack> {
        crate::protocol::workstation::WorkstationView::decode(public).map_or_else(Vec::new, |v| {
            self.allowed(false)
                .iter()
                .filter_map(|i| v.slots.get(*i as usize)?.clone())
                .collect()
        })
    }
    fn accepts(&self, public: &[u8], stack: &Stack, _: &Catalog) -> bool {
        crate::protocol::workstation::WorkstationView::decode(public).is_some_and(|mut v| {
            self.allowed(true).iter().any(|i| {
                self.accepts_slot(*i as usize, stack)
                    && v.slots.get_mut(*i as usize).is_some_and(|s| put(s, stack))
            })
        })
    }
    fn withdraw(
        &self,
        p: &EntityPayload,
        item: crate::items::ItemId,
        count: u16,
        _: &Catalog,
    ) -> Result<Option<(EntityPayload, Stack)>, EntityError> {
        let mut p = p
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        for slot in self.allowed(false) {
            let i = slot as usize;
            if p.slots[i]
                .as_ref()
                .is_some_and(|s| s.item == item && s.count >= count)
            {
                let stack = take(&mut p.slots[i], count).ok_or(EntityError::InvalidPayload)?;
                self.reset_input(&mut p, i, true);
                return Ok(Some((EntityPayload::new(p), stack)));
            }
        }
        Ok(None)
    }
    fn deposit(
        &self,
        p: &EntityPayload,
        stack: &Stack,
        _: &Catalog,
    ) -> Result<Option<EntityPayload>, EntityError> {
        let mut p = p
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        for slot in self.allowed(true) {
            let i = slot as usize;
            if self.accepts_slot(i, stack) && put(&mut p.slots[i], stack) {
                self.reset_input(&mut p, i, false);
                return Ok(Some(EntityPayload::new(p)));
            }
        }
        Ok(None)
    }
}
impl EntityInteractionPolicy for Adapter {
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        catalog: &Catalog,
        _: &VoxelView,
        _: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        // Version 1 is the existing fixed hotkey request; version 2 is the
        // generic screen request and additionally fences identity and revision.
        if !((request.len() == 6 && request[0] == 1)
            || (request.len() == 22
                && request[0] == 2
                && u64::from_le_bytes(request[6..14].try_into().unwrap()) == snapshot.id.get()
                && u64::from_le_bytes(request[14..22].try_into().unwrap()) == snapshot.revision))
        {
            return Err(EntityError::InvalidPayload);
        }
        let slot = request[2] as usize;
        let player = request[3] as usize;
        let count = u16::from_le_bytes([request[4], request[5]]);
        if slot >= self.definition.slots as usize
            || player >= crate::inventory::SLOTS
            || !(1..=128).contains(&count)
            || request[1] > 1
        {
            return Err(EntityError::InvalidPayload);
        }
        let group = catalog
            .inventory_screen(snapshot.entity_type)
            .and_then(|s| s.group(slot as u8))
            .ok_or(EntityError::InvalidPayload)?;
        let before = snapshot
            .private_payload
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let mut p = before.clone();
        let mut inventory = inventory.clone();
        let insert = request[1] == 0;
        if insert {
            if !group.insert {
                return Err(EntityError::InvalidPayload);
            }
            let stack =
                take(&mut inventory.slots[player], count).ok_or(EntityError::InvalidPayload)?;
            if !self.accepts_slot(slot, &stack) || !put(&mut p.slots[slot], &stack) {
                return Err(EntityError::InvalidPayload);
            }
        } else {
            if !group.extract {
                return Err(EntityError::InvalidPayload);
            }
            let stack = take(&mut p.slots[slot], count).ok_or(EntityError::InvalidPayload)?;
            if !put(&mut inventory.slots[player], &stack) {
                return Err(EntityError::InvalidPayload);
            }
        }
        self.reset_input(&mut p, slot, !insert);
        inventory.revision = inventory
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let block_states = self.block_states(snapshot, before, &p)?;
        self.encode(&EntityPayload::new(p.clone()))
            .map_err(|_| EntityError::InvalidPayload)?;
        Ok(EntityInteractionPlan {
            payload: EntityPayload::new(p),
            inventory,
            block_states,
            wakes: vec![],
        })
    }
}
