//! Declarative lifecycle effects. Storage is host-owned: placement initializes
//! empty slots; removal releases the exact authoritative contents and one refund.
use crate::RegistrationError;

pub const MAX_FOOTPRINT: usize = 64;
// Fits full component-bearing snapshots within the host's 64 KiB entity bound.
pub const MAX_STORAGE_SLOTS: usize = crate::inventory::MAX_SLOTS;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintCell {
    pub offset: [i32; 3],
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct StorageBlockEntity {
    pub entity: String,
    pub block: String,
    pub placement_item: String,
    pub anchor_state: String,
    pub footprint: Vec<FootprintCell>,
    pub slots: usize,
    /// Outward cardinal normals accessible to automation. `None` retains the
    /// existing all-faces behavior for storage declarations.
    pub automation_faces: Option<Vec<[i32; 3]>>,
}

/// No mutable world, engine client object, or journal access crosses the API.
pub struct PlacementContext<'a> {
    pub anchor: [i32; 3],
    pub state: &'a str,
}

pub struct RemovalContext<'a> {
    pub anchor: [i32; 3],
    pub touched: [i32; 3],
    pub footprint: &'a [[i32; 3]],
}

#[derive(Debug)]
pub struct PlaceStorage {
    pub cells: Vec<([i32; 3], String)>,
    pub consume_item: String,
    pub empty_slots: usize,
}

#[derive(Debug)]
pub struct RemoveStorage {
    pub cells: Vec<([i32; 3], String)>,
    pub refund_item: String,
    // Contents are deliberately not supplied by extension code: the host takes
    // them from the revision-checked entity in the same removal transaction.
}

impl StorageBlockEntity {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let invalid = |message: &str| RegistrationError(message.into());
        if self.slots == 0 || self.slots > MAX_STORAGE_SLOTS {
            return Err(invalid("storage capacity must be 1..=54"));
        }
        if let Some(faces) = &self.automation_faces {
            let mut unique = std::collections::BTreeSet::new();
            if faces.is_empty()
                || faces.len() > crate::machine::FACES.len()
                || faces
                    .iter()
                    .any(|face| !crate::machine::FACES.contains(face) || !unique.insert(*face))
            {
                return Err(invalid(
                    "storage automation faces must be distinct cardinal normals",
                ));
            }
        }
        if self.footprint.is_empty() || self.footprint.len() > MAX_FOOTPRINT {
            return Err(invalid("footprint must contain 1..=64 cells"));
        }
        let mut offsets = std::collections::BTreeSet::new();
        for cell in &self.footprint {
            if cell.offset.iter().any(|v| v.unsigned_abs() > 16) || !offsets.insert(cell.offset) {
                return Err(invalid("duplicate or out-of-range footprint offset"));
            }
        }
        if !self
            .footprint
            .iter()
            .any(|c| c.offset == [0; 3] && c.state == self.anchor_state)
        {
            return Err(invalid("footprint must contain its declared anchor state"));
        }
        Ok(())
    }

    pub fn allowed_automation_faces(&self) -> &[[i32; 3]] {
        self.automation_faces
            .as_deref()
            .unwrap_or(&crate::machine::FACES)
    }

    pub fn plan_place(
        &self,
        context: PlacementContext<'_>,
    ) -> Result<PlaceStorage, RegistrationError> {
        self.validate()?;
        if context.state != self.anchor_state {
            return Err(RegistrationError("invalid placement state".into()));
        }
        let cells = self
            .footprint
            .iter()
            .map(|cell| {
                let mut at = context.anchor;
                for (value, offset) in at.iter_mut().zip(cell.offset) {
                    *value = value
                        .checked_add(offset)
                        .ok_or_else(|| RegistrationError("footprint coordinate overflow".into()))?;
                }
                Ok((at, cell.state.clone()))
            })
            .collect::<Result<Vec<_>, RegistrationError>>()?;
        Ok(PlaceStorage {
            cells,
            consume_item: self.placement_item.clone(),
            empty_slots: self.slots,
        })
    }

    pub fn plan_remove(
        &self,
        context: RemovalContext<'_>,
    ) -> Result<RemoveStorage, RegistrationError> {
        let placed = self.plan_place(PlacementContext {
            anchor: context.anchor,
            state: &self.anchor_state,
        })?;
        let expected: std::collections::BTreeSet<_> =
            placed.cells.iter().map(|(cell, _)| *cell).collect();
        if context.footprint.len() != expected.len()
            || context
                .footprint
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != expected
            || !expected.contains(&context.touched)
        {
            return Err(RegistrationError(
                "stored footprint differs from lifecycle declaration".into(),
            ));
        }
        Ok(RemoveStorage {
            cells: placed.cells,
            refund_item: self.placement_item.clone(),
        })
    }
}
