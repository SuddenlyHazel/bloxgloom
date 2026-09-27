use super::*;
use crate::inventory::InventoryStore;

pub(in crate::server::entities) struct Codec<const N: usize> {
    pub catalog: Arc<Catalog>,
    pub kind: WorkstationKind,
}
impl<const N: usize> EntityPayloadCodec for Codec<N> {
    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityCodecError::InvalidData)?;
        let mut inventory = Inventory::default();
        inventory.slots[..N].clone_from_slice(&payload.slots);
        InventoryStore::encode_snapshot_with_catalog(&inventory, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > N * 1100 + 128 {
            return Err(EntityCodecError::InvalidData);
        }
        let inventory = InventoryStore::decode_snapshot_with_catalog(bytes, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if inventory.revision != 0 || inventory.slots[N..].iter().any(Option::is_some) {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(StoragePayload::<N> {
            slots: std::array::from_fn(|i| inventory.slots[i].clone()),
        }))
    }
    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityCodecError::InvalidData)?;
        Ok(crate::protocol::workstation::WorkstationView {
            kind: self.kind,
            slots: payload.slots.to_vec(),
            ..Default::default()
        }
        .encode())
    }
}
