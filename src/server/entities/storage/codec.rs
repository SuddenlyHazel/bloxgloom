use super::*;
pub(in crate::server::entities) struct Codec<const N: usize> {
    pub catalog: Arc<Catalog>,
}
impl<const N: usize> EntityPayloadCodec for Codec<N> {
    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityCodecError::InvalidData)?;
        crate::inventory::container::encode(&payload.slots, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        let slots = crate::inventory::container::decode(bytes, N, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        Ok(EntityPayload::new(StoragePayload::<N> {
            slots: slots
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        }))
    }
    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<StoragePayload<N>>()
            .ok_or(EntityCodecError::InvalidData)?;
        Ok(crate::protocol::workstation::WorkstationView {
            slots: payload.slots.to_vec(),
            status: vec![],
        }
        .encode())
    }
}
