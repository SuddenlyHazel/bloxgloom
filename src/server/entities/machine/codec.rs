use super::*;
impl Adapter {
    fn validate(&self, p: &MachinePayload) -> Result<(), EntityCodecError> {
        if p.variant as usize >= self.definition.variants.len()
            || p.slots.len() != self.definition.slots as usize
            || p.data.len() > 1024
            || p.fuel > 240
            || p.progress > 60000
            || p.slots
                .iter()
                .enumerate()
                .any(|(i, s)| s.as_ref().is_some_and(|s| !self.accepts_slot(i, s)))
        {
            return Err(EntityCodecError::InvalidData);
        }
        if let Some(process) = &self.definition.process {
            let input = p.slots[process.input as usize]
                .as_ref()
                .filter(|s| s.components.is_none())
                .map(|s| s.item);
            if (p.progress > 0 && (input.is_none() || input != p.progress_item))
                || p.progress_item.is_some_and(|i| Some(i) != input)
                || process.fuel.is_none() && p.fuel != 0
            {
                return Err(EntityCodecError::InvalidData);
            }
            if p.progress > 0 && self.recipe(input).is_none_or(|r| p.progress >= r.pulses) {
                return Err(EntityCodecError::InvalidData);
            }
            if p.fuel > process.fuels.iter().map(|f| f.pulses).max().unwrap_or(0) {
                return Err(EntityCodecError::InvalidData);
            }
        } else if p.fuel != 0 || p.progress != 0 || p.progress_item.is_some() {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(())
    }
}
impl EntityPayloadCodec for Adapter {
    fn encode(&self, p: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let p = p
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        self.validate(p)?;
        let mut b = vec![1, p.variant];
        b.extend(p.fuel.to_le_bytes());
        b.extend(p.progress.to_le_bytes());
        b.extend(p.progress_item.map_or(0, |i| i.0).to_le_bytes());
        b.extend((p.data.len() as u16).to_le_bytes());
        b.extend(&p.data);
        b.extend(
            crate::inventory::container::encode(&p.slots, &self.catalog)
                .map_err(|_| EntityCodecError::InvalidData)?,
        );
        Ok(b)
    }
    fn decode(&self, b: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if b.len() < 12 || b[0] != 1 {
            return Err(EntityCodecError::InvalidData);
        }
        let len = u16::from_le_bytes(b[10..12].try_into().unwrap()) as usize;
        if len > 1024 || b.len() < 12 + len {
            return Err(EntityCodecError::InvalidData);
        }
        let item = u32::from_le_bytes(b[6..10].try_into().unwrap());
        let p = MachinePayload {
            variant: b[1],
            fuel: u16::from_le_bytes(b[2..4].try_into().unwrap()),
            progress: u16::from_le_bytes(b[4..6].try_into().unwrap()),
            progress_item: (item != 0).then_some(crate::items::ItemId(item)),
            data: b[12..12 + len].to_vec(),
            slots: crate::inventory::container::decode(
                &b[12 + len..],
                self.definition.slots as usize,
                &self.catalog,
            )
            .map_err(|_| EntityCodecError::InvalidData)?,
        };
        self.validate(&p)?;
        Ok(EntityPayload::new(p))
    }
    fn public_view(&self, p: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let p = p
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        self.validate(p)?;
        let status = if self.definition.process.is_some() {
            let duration = self.recipe(p.progress_item).map_or(1, |r| r.pulses);
            vec![
                u32::from(p.fuel) * self.definition.interval * 20,
                u32::from(p.progress) * 1000 / u32::from(duration),
            ]
        } else {
            vec![]
        };
        Ok(crate::protocol::workstation::WorkstationView {
            slots: p.slots.clone(),
            status,
        }
        .encode())
    }
}
