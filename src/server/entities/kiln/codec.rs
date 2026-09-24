//! Bounded canonical kiln payload codec and privacy-safe public projection.

use super::super::codec::{Decoder, Encoder};
use super::super::registry::{EntityCodecError, EntityPayloadCodec};
use super::super::types::EntityPayload;
use super::model::{KILN_MAX_COOK_TICKS, KILN_MAX_PAYLOAD_BYTES, KilnFacing, KilnPayload};
use crate::content::Catalog;
use crate::inventory::{ComponentPayload, MAX_COMPONENT_BYTES, Stack};
use crate::items::ItemId;
use std::sync::Arc;

const KILN_PAYLOAD_VERSION: u8 = 1;

pub(super) struct KilnPayloadCodec {
    pub(super) catalog: Arc<Catalog>,
}

impl EntityPayloadCodec for KilnPayloadCodec {
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > KILN_MAX_PAYLOAD_BYTES {
            return Err(EntityCodecError::InvalidData);
        }
        let mut decoder = Decoder::new(bytes);
        if decoder.u8().map_err(|_| EntityCodecError::InvalidData)? != KILN_PAYLOAD_VERSION {
            return Err(EntityCodecError::UnsupportedVersion);
        }
        let facing = KilnFacing::decode(decoder.u8().map_err(|_| EntityCodecError::InvalidData)?)?;
        let lit = match decoder.u8().map_err(|_| EntityCodecError::InvalidData)? {
            0 => false,
            1 => true,
            _ => return Err(EntityCodecError::InvalidData),
        };
        let fuel_remaining = decoder.u16().map_err(|_| EntityCodecError::InvalidData)?;
        let cook_progress = decoder.u16().map_err(|_| EntityCodecError::InvalidData)?;
        let progress_item = match decoder.u8().map_err(|_| EntityCodecError::InvalidData)? {
            0 => None,
            1 => Some(ItemId(
                decoder.u32().map_err(|_| EntityCodecError::InvalidData)?,
            )),
            _ => return Err(EntityCodecError::InvalidData),
        };
        let mut slots: [Option<Stack>; 3] = std::array::from_fn(|_| None);
        for slot in &mut slots {
            *slot = decode_stack(&mut decoder)?;
        }
        decoder
            .finish()
            .map_err(|_| EntityCodecError::InvalidData)?;
        let payload = KilnPayload {
            facing,
            lit,
            fuel_remaining,
            cook_progress,
            progress_item,
            slots,
        };
        payload
            .validate(&self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        let encoded = encode_payload(&payload, &self.catalog)?;
        if encoded != bytes {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(payload))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        encode_payload(payload, &self.catalog)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        payload
            .validate(&self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        let progress = (u32::from(payload.cook_progress) * u32::from(u8::MAX)
            / u32::from(KILN_MAX_COOK_TICKS)) as u8;
        Ok(vec![
            payload.facing.encoded(),
            u8::from(payload.lit),
            progress,
        ])
    }
}

fn encode_payload(payload: &KilnPayload, catalog: &Catalog) -> Result<Vec<u8>, EntityCodecError> {
    payload
        .validate(catalog)
        .map_err(|_| EntityCodecError::InvalidData)?;
    let mut encoder = Encoder::with_capacity(256);
    encoder.u8(KILN_PAYLOAD_VERSION);
    encoder.u8(payload.facing.encoded());
    encoder.u8(u8::from(payload.lit));
    encoder.u16(payload.fuel_remaining);
    encoder.u16(payload.cook_progress);
    match payload.progress_item {
        Some(item) => {
            encoder.u8(1);
            encoder.u32(item.0);
        }
        None => encoder.u8(0),
    }
    for stack in &payload.slots {
        encode_stack(&mut encoder, stack)?;
    }
    if encoder.len() > KILN_MAX_PAYLOAD_BYTES {
        return Err(EntityCodecError::InvalidData);
    }
    // Generic entity checkpoint/WAL frames provide the outer checksum.
    let encoded = encoder
        .finish_crc()
        .map_err(|_| EntityCodecError::InvalidData)?;
    Ok(encoded[..encoded.len() - 4].to_vec())
}

fn encode_stack(encoder: &mut Encoder, stack: &Option<Stack>) -> Result<(), EntityCodecError> {
    let Some(stack) = stack else {
        encoder.u8(0);
        return Ok(());
    };
    encoder.u8(1);
    encoder.u32(stack.item.0);
    encoder.u16(stack.count);
    match &stack.components {
        None => {
            encoder.u16(0);
            encoder.u16(0);
        }
        Some(components) => {
            if components.bytes.len() > MAX_COMPONENT_BYTES
                || components.bytes.is_empty()
                || components.version == 0
            {
                return Err(EntityCodecError::InvalidData);
            }
            encoder.u16(components.version);
            encoder.u16(
                u16::try_from(components.bytes.len()).map_err(|_| EntityCodecError::InvalidData)?,
            );
            encoder.raw(&components.bytes);
        }
    }
    Ok(())
}

fn decode_stack(decoder: &mut Decoder<'_>) -> Result<Option<Stack>, EntityCodecError> {
    match decoder.u8().map_err(|_| EntityCodecError::InvalidData)? {
        0 => Ok(None),
        1 => {
            let item = ItemId(decoder.u32().map_err(|_| EntityCodecError::InvalidData)?);
            let count = decoder.u16().map_err(|_| EntityCodecError::InvalidData)?;
            let version = decoder.u16().map_err(|_| EntityCodecError::InvalidData)?;
            let length = usize::from(decoder.u16().map_err(|_| EntityCodecError::InvalidData)?);
            if length > MAX_COMPONENT_BYTES {
                return Err(EntityCodecError::InvalidData);
            }
            let components = if length == 0 {
                if version != 0 {
                    return Err(EntityCodecError::InvalidData);
                }
                None
            } else {
                Some(Arc::new(
                    ComponentPayload::new(
                        version,
                        decoder
                            .raw(length)
                            .map_err(|_| EntityCodecError::InvalidData)?
                            .to_vec(),
                    )
                    .ok_or(EntityCodecError::InvalidData)?,
                ))
            };
            Ok(Some(Stack {
                item,
                count,
                components,
            }))
        }
        _ => Err(EntityCodecError::InvalidData),
    }
}
