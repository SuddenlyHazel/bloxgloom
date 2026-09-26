use super::*;

pub(super) struct Codec;
impl EntityPayloadCodec for Codec {
    fn validate_location(&self, location: &EntityLocation) -> Result<(), EntityError> {
        match location {
            EntityLocation::Mobile { position } if terrain::valid_position(*position) => Ok(()),
            _ => Err(EntityError::InvalidLocation),
        }
    }

    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() != 41 || bytes[14] > 1 {
            return Err(EntityCodecError::InvalidData);
        }
        let cell = |offset: usize| -> Result<Option<[i32; 2]>, EntityCodecError> {
            let value = [
                i32::from_le_bytes(bytes[offset + 1..offset + 5].try_into().unwrap()),
                i32::from_le_bytes(bytes[offset + 5..offset + 9].try_into().unwrap()),
            ];
            match bytes[offset] {
                0 if value == [0; 2] => Ok(None),
                1 => Ok(Some(value)),
                _ => Err(EntityCodecError::InvalidData),
            }
        };
        let payload = EntityPayload::new(Mossbun {
            cycle: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            facing: bytes[8],
            steps: bytes[9],
            vertical_velocity: f32::from_le_bytes(bytes[10..14].try_into().unwrap()),
            grounded: bytes[14] == 1,
            goal: cell(15)?,
            waypoint: cell(24)?,
            think_at: u64::from_le_bytes(bytes[33..41].try_into().unwrap()),
        });
        self.encode(&payload)?;
        Ok(payload)
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bun = payload
            .downcast_ref::<Mossbun>()
            .ok_or(EntityCodecError::InvalidData)?;
        if bun.facing > 3
            || bun.steps > 16
            || !bun.vertical_velocity.is_finite()
            || !(-24.0..=0.0).contains(&bun.vertical_velocity)
            || [bun.goal, bun.waypoint]
                .into_iter()
                .flatten()
                .flatten()
                .any(|v| !(-999_999..999_999).contains(&v))
        {
            return Err(EntityCodecError::InvalidData);
        }
        let mut bytes = bun.cycle.to_le_bytes().to_vec();
        bytes.extend([bun.facing, bun.steps]);
        bytes.extend(bun.vertical_velocity.to_le_bytes());
        bytes.push(u8::from(bun.grounded));
        for cell in [bun.goal, bun.waypoint] {
            bytes.push(u8::from(cell.is_some()));
            for value in cell.unwrap_or([0; 2]) {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend(bun.think_at.to_le_bytes());
        Ok(bytes)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(payload)?;
        let bun = payload.downcast_ref::<Mossbun>().unwrap();
        Ok(vec![
            bun.facing,
            u8::from(bun.waypoint.is_some()) | (u8::from(bun.grounded) << 1),
        ])
    }
}
