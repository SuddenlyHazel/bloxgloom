//! Atomic owned-inventory processing. Validate every input/output before
//! publishing any candidate payload; stale inputs and full outputs do no work.
use super::super::transfer::{put, take};
use super::*;

impl Adapter {
    fn stack_value(&self, value: &api::StackValue) -> Result<Stack, EntityError> {
        if !value.valid() {
            return Err(EntityError::InvalidPayload);
        }
        let mut stack = Stack::new(self.item(&value.item)?, value.count);
        stack.components = value
            .components
            .as_ref()
            .map(|value| {
                crate::inventory::ComponentPayload::new(value.version, value.bytes.clone())
                    .map(Arc::new)
                    .ok_or(EntityError::InvalidPayload)
            })
            .transpose()?;
        if !stack.valid_in(&self.catalog) {
            return Err(EntityError::InvalidPayload);
        }
        Ok(stack)
    }

    pub(super) fn transform(
        &self,
        payload: &mut MachinePayload,
        work: &api::Transformation,
    ) -> Result<bool, EntityError> {
        if !work.valid(payload.slots.len()) {
            return Err(EntityError::InvalidPayload);
        }
        let mut candidate = payload.clone();
        for input in &work.inputs {
            let expected = self.stack_value(&input.expected)?;
            let slot = usize::from(input.slot);
            if candidate.slots[slot].as_ref() != Some(&expected) {
                return Ok(false);
            }
            take(&mut candidate.slots[slot], input.count).ok_or(EntityError::InvalidPayload)?;
            self.reset_input(&mut candidate, slot, true);
        }
        for output in &work.outputs {
            let stack = self.stack_value(&output.stack)?;
            let slot = usize::from(output.slot);
            if !self.accepts_slot(slot, &stack) {
                return Err(EntityError::InvalidPayload);
            }
            if !put(&mut candidate.slots[slot], &stack) {
                return Ok(false);
            }
            self.reset_input(&mut candidate, slot, false);
        }
        *payload = candidate;
        Ok(true)
    }
}
