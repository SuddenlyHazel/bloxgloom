//! Public machine footprint decisions. The host still validates terrain,
//! inventory costs, stored entities, and the atomic workstation transaction.
use super::Machine;
use crate::{FootprintCell, RegistrationError};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LifecyclePlan {
    pub cells: Vec<([i32; 3], String)>,
    pub item: String,
    pub variant: u8,
}

impl Machine {
    /// Resolve a placement state to its declared variant and idle footprint.
    /// Definitions are validated when registered; runtime coordinate overflow
    /// is still rejected before any world or inventory changes are staged.
    pub fn plan_place(
        &self,
        anchor: [i32; 3],
        placement_state: &str,
    ) -> Result<LifecyclePlan, RegistrationError> {
        let variant = self
            .variants
            .iter()
            .position(|value| value.placement_state == placement_state)
            .ok_or_else(|| RegistrationError("machine placement state is not a variant".into()))?;
        self.footprint(anchor, variant, false)
    }

    /// Resolve the current variant and verify that the persisted footprint
    /// still belongs to this machine before planning a removal/refund.
    pub fn plan_remove(
        &self,
        anchor: [i32; 3],
        touched: [i32; 3],
        variant: u8,
        active: bool,
        stored_footprint: &[[i32; 3]],
    ) -> Result<LifecyclePlan, RegistrationError> {
        let plan = self.footprint(anchor, usize::from(variant), active)?;
        let expected: BTreeSet<_> = plan.cells.iter().map(|(cell, _)| *cell).collect();
        let stored: BTreeSet<_> = stored_footprint.iter().copied().collect();
        if stored_footprint.len() != expected.len()
            || stored != expected
            || !expected.contains(&touched)
        {
            return Err(RegistrationError(
                "stored machine footprint differs from declaration".into(),
            ));
        }
        Ok(plan)
    }

    fn footprint(
        &self,
        anchor: [i32; 3],
        variant: usize,
        active: bool,
    ) -> Result<LifecyclePlan, RegistrationError> {
        let value = self
            .variants
            .get(variant)
            .ok_or_else(|| RegistrationError("invalid machine variant".into()))?;
        let footprint: &[FootprintCell] = if active { &value.active } else { &value.idle };
        let mut cells = footprint
            .iter()
            .map(|part| {
                let mut cell = anchor;
                for (coordinate, offset) in cell.iter_mut().zip(part.offset) {
                    *coordinate = coordinate.checked_add(offset).ok_or_else(|| {
                        RegistrationError("machine footprint coordinate overflow".into())
                    })?;
                }
                Ok((cell, part.state.clone()))
            })
            .collect::<Result<Vec<_>, RegistrationError>>()?;
        cells.sort_by_key(|(cell, _)| *cell);
        Ok(LifecyclePlan {
            cells,
            item: self.item.clone(),
            variant: u8::try_from(variant)
                .map_err(|_| RegistrationError("machine variant index exceeds u8".into()))?,
        })
    }
}
