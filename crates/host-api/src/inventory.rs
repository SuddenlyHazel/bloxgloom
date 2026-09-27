//! Bounded inventory presentation and access declarations shared by both peers.
use crate::RegistrationError;
pub const MAX_SLOTS: usize = 54;
pub const MAX_STATUS_FIELDS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotGroup {
    pub label: String,
    pub first: u8,
    pub count: u8,
    pub insert: bool,
    pub extract: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusFormat {
    Number,
    Milliseconds,
    Progress,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusField {
    pub label: String,
    pub format: StatusFormat,
    pub maximum: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryScreen {
    pub entity: String,
    pub block: String,
    pub title: String,
    pub hint: String,
    pub slots: u8,
    pub columns: u8,
    pub footprint: Vec<[i32; 3]>,
    pub groups: Vec<SlotGroup>,
    pub status: Vec<StatusField>,
}
impl InventoryScreen {
    pub fn storage(
        entity: &str,
        block: &str,
        title: &str,
        slots: u8,
        columns: u8,
        footprint: Vec<[i32; 3]>,
    ) -> Self {
        Self {
            entity: entity.into(),
            block: block.into(),
            title: title.into(),
            hint: String::new(),
            slots,
            columns,
            footprint,
            groups: vec![SlotGroup {
                label: "STORAGE".into(),
                first: 0,
                count: slots,
                insert: true,
                extract: true,
            }],
            status: vec![],
        }
    }
    pub fn group(&self, slot: u8) -> Option<&SlotGroup> {
        self.groups.iter().find(|g| {
            slot >= g.first && usize::from(slot) < usize::from(g.first) + usize::from(g.count)
        })
    }
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let text = |s: &str, max: usize| {
            s.len() <= max && s.bytes().all(|c| c.is_ascii_graphic() || c == b' ')
        };
        if self.slots == 0
            || usize::from(self.slots) > MAX_SLOTS
            || !(1..=9).contains(&self.columns)
            || self.slots.div_ceil(self.columns) > 6
            || !text(&self.title, 40)
            || self.title.is_empty()
            || !text(&self.hint, 80)
            || self.groups.is_empty()
            || self.groups.len() > MAX_SLOTS
            || self.status.len() > MAX_STATUS_FIELDS
            || self.footprint.is_empty()
            || self.footprint.len() > crate::lifecycle::MAX_FOOTPRINT
        {
            return Err(RegistrationError("invalid inventory screen bounds".into()));
        }
        let offsets: std::collections::BTreeSet<_> = self.footprint.iter().copied().collect();
        if offsets.len() != self.footprint.len()
            || !offsets.contains(&[0; 3])
            || offsets.iter().flatten().any(|v| v.unsigned_abs() > 16)
        {
            return Err(RegistrationError(
                "invalid inventory screen footprint".into(),
            ));
        }
        let mut covered = vec![false; usize::from(self.slots)];
        for group in &self.groups {
            let end = usize::from(group.first) + usize::from(group.count);
            if group.count == 0 || end > covered.len() || !text(&group.label, 20) {
                return Err(RegistrationError("invalid slot group".into()));
            }
            for slot in &mut covered[usize::from(group.first)..end] {
                if *slot {
                    return Err(RegistrationError("overlapping slot groups".into()));
                }
                *slot = true;
            }
        }
        if covered.contains(&false)
            || self
                .status
                .iter()
                .any(|s| !text(&s.label, 16) || s.maximum == 0)
        {
            return Err(RegistrationError("incomplete inventory view".into()));
        }
        Ok(())
    }
    /// Canonical descriptor bytes for schema/handshake fingerprints, not a UI
    /// download protocol. Both peers install the same content package.
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut out = vec![1, self.slots, self.columns];
        for text in [&self.entity, &self.block, &self.title, &self.hint] {
            out.extend((text.len() as u32).to_le_bytes());
            out.extend(text.as_bytes());
        }
        out.push(self.footprint.len() as u8);
        for cell in &self.footprint {
            for v in cell {
                out.extend(v.to_le_bytes());
            }
        }
        out.push(self.groups.len() as u8);
        for group in &self.groups {
            out.extend([
                group.first,
                group.count,
                u8::from(group.insert),
                u8::from(group.extract),
                group.label.len() as u8,
            ]);
            out.extend(group.label.as_bytes());
        }
        out.push(self.status.len() as u8);
        for status in &self.status {
            out.push(match status.format {
                StatusFormat::Number => 0,
                StatusFormat::Milliseconds => 1,
                StatusFormat::Progress => 2,
            });
            out.extend(status.maximum.to_le_bytes());
            out.push(status.label.len() as u8);
            out.extend(status.label.as_bytes());
        }
        out
    }
}
