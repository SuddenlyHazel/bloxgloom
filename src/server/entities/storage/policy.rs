use super::super::transfer::{put, take};
use super::*;
use crate::items::ItemId;
use crate::protocol::workstation::WorkstationView;
use crate::server::voxel_view::VoxelView;

fn public_slots(public: &[u8]) -> Option<Vec<Option<Stack>>> {
    if let Some(view) = WorkstationView::decode(public) {
        return Some(view.slots);
    }
    if public.first() != Some(&4) {
        return None;
    }
    let count = usize::from(*public.get(1)?);
    if count == 0
        || count > bloxgloom_host_api::lifecycle::MAX_STORAGE_SLOTS
        || public.len() != 2 + count * 6
    {
        return None;
    }
    public[2..]
        .chunks_exact(6)
        .map(|bytes| {
            let item = ItemId(u32::from_le_bytes(bytes[..4].try_into().ok()?));
            let count = u16::from_le_bytes(bytes[4..].try_into().ok()?);
            if count > STACK_LIMIT || (item.0 == 0) != (count == 0) {
                return None;
            }
            Some((count != 0).then(|| Stack::new(item, count)))
        })
        .collect()
}

pub(in crate::server::entities) struct Port<P>(std::marker::PhantomData<P>);
impl<P> Port<P> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}
impl<P: Slots> EntityTransferPolicy for Port<P> {
    fn offers(&self, public: &[u8]) -> Vec<Stack> {
        public_slots(public).map_or_else(Vec::new, |slots| slots.into_iter().flatten().collect())
    }
    fn accepts(&self, public: &[u8], stack: &Stack, _: &Catalog) -> bool {
        public_slots(public).is_some_and(|mut slots| slots.iter_mut().any(|slot| put(slot, stack)))
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
        for slot in payload.slots_mut() {
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
            .downcast_ref::<P>()
            .ok_or(EntityError::InvalidPayload)?
            .clone();
        if payload.slots_mut().iter_mut().any(|slot| put(slot, stack)) {
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
        _: &Catalog,
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
