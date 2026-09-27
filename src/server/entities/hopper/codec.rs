use super::*;
use crate::inventory::InventoryStore;

pub(super) struct Codec {
    pub catalog: Arc<Catalog>,
}
impl EntityPayloadCodec for Codec {
    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<HopperPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        let mut inventory = Inventory::default();
        inventory.slots[..3].clone_from_slice(&payload.slots);
        InventoryStore::encode_snapshot_with_catalog(&inventory, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > 4096 {
            return Err(EntityCodecError::InvalidData);
        }
        let inventory = InventoryStore::decode_snapshot_with_catalog(bytes, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if inventory.revision != 0 || inventory.slots[3..].iter().any(Option::is_some) {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(HopperPayload {
            slots: std::array::from_fn(|i| inventory.slots[i].clone()),
        }))
    }
    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<HopperPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        Ok(crate::protocol::workstation::WorkstationView {
            hopper: true,
            slots: payload.slots.clone(),
            ..Default::default()
        }
        .encode())
    }
}
