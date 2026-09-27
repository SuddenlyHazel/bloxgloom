//! Declarative component operations. Values are registration constants, never
//! private stack data exposed to a behavior. The host evaluates predicates.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComponentValue {
    pub version: u16,
    pub bytes: Vec<u8>,
}
impl ComponentValue {
    pub fn valid(&self) -> bool {
        self.version != 0 && !self.bytes.is_empty() && self.bytes.len() <= 1024
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentMatch {
    #[default]
    Empty,
    /// Any nonempty component payload, including versions unknown to the mod.
    Present,
    Exact(ComponentValue),
}
impl ComponentMatch {
    pub fn valid(&self) -> bool {
        match self {
            Self::Exact(v) => v.valid(),
            _ => true,
        }
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Empty, _) | (_, Self::Empty) => false,
            (Self::Present, _) | (_, Self::Present) => true,
            (Self::Exact(a), Self::Exact(b)) => a == b,
        }
    }
    pub(super) fn fingerprint(&self, out: &mut Vec<u8>) {
        match self {
            Self::Empty => out.push(0),
            Self::Present => out.push(1),
            Self::Exact(v) => {
                out.push(2);
                value_bytes(v, out);
            }
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ComponentOutput {
    #[default]
    Empty,
    /// Copy the entire exact input payload, not just its public identity.
    PreserveInput,
    Exact(ComponentValue),
}
impl ComponentOutput {
    pub fn valid(&self) -> bool {
        match self {
            Self::Exact(v) => v.valid(),
            _ => true,
        }
    }
    pub(super) fn fingerprint(&self, out: &mut Vec<u8>) {
        match self {
            Self::Empty => out.push(0),
            Self::PreserveInput => out.push(1),
            Self::Exact(v) => {
                out.push(2);
                value_bytes(v, out);
            }
        }
    }
}
fn value_bytes(v: &ComponentValue, out: &mut Vec<u8>) {
    out.extend(v.version.to_le_bytes());
    out.extend((v.bytes.len() as u32).to_le_bytes());
    out.extend(&v.bytes);
}
