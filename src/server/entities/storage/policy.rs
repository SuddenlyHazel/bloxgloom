use super::super::transfer::{put, take};
use super::*;
use crate::items::ItemId;
use crate::protocol::workstation::WorkstationView;
use crate::server::voxel_view::VoxelView;

pub(in crate::server::entities) struct Port<const N: usize>;
impl<const N: usize> EntityTransferPolicy for Port<N> {
    fn offers(&self, public: &[u8]) -> Vec<Stack> {
        WorkstationView::decode(public)
            .map_or_else(Vec::new, |v| v.slots.into_iter().flatten().collect())
    }
    fn accepts(&self, public: &[u8], stack: &Stack, _: &Catalog) -> bool {
        WorkstationView::decode(public)
            .is_some_and(|mut v| v.slots.iter_mut().any(|slot| put(slot, stack)))
    }
    fn withdraw(
        &self,
        payload: &EntityPayload,
        item: ItemId,
        count: u16,
        _: &Catalog,
    ) -> Result<Option<(EntityPayload, Stack)>, EntityError> {
        let mut payload = payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        for slot in &mut payload.slots {
            if slot
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
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        if payload.slots.iter_mut().any(|slot| put(slot, stack)) {
            Ok(Some(EntityPayload::new(payload)))
        } else {
            Ok(None)
        }
    }
}

pub(in crate::server::entities) struct Interaction<const N: usize>;
fn block(snapshot: &EntitySnapshot) -> Result<Vec<EntityBlockStateChange>, EntityError> {
    let EntityLocation::Anchored { anchor_state, .. } = snapshot.location else {
        return Err(EntityError::WrongOwnership);
    };
    Ok(vec![EntityBlockStateChange {
        cell: snapshot.anchor().ok_or(EntityError::WrongOwnership)?,
        before: anchor_state,
        after: anchor_state,
    }])
}
impl<const N: usize> EntityInteractionPolicy for Interaction<N> {
    fn reads_neighbours(&self) -> bool {
        false
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        request: &[u8],
        inventory: &Inventory,
        _: &Catalog,
        _: &VoxelView,
        _: &EntityView,
    ) -> Result<EntityInteractionPlan, EntityError> {
        if request.len() != 22
            || request[0] != 2
            || request[1] > 1
            || usize::from(request[2]) >= N
            || usize::from(request[3]) >= crate::inventory::SLOTS
            || u64::from_le_bytes(request[6..14].try_into().unwrap()) != snapshot.id.get()
            || u64::from_le_bytes(request[14..22].try_into().unwrap()) != snapshot.revision
        {
            return Err(EntityError::InvalidPayload);
        }
        let count = u16::from_le_bytes([request[4], request[5]]);
        if count == 0 || count > STACK_LIMIT {
            return Err(EntityError::InvalidPayload);
        }
        let mut payload = snapshot
            .private_payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        let mut inventory = inventory.clone();
        let machine = &mut payload.slots[request[2] as usize];
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
            block_states: block(snapshot)?,
            wakes: vec![],
        })
    }
}
