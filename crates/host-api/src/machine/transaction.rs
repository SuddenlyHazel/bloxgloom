//! Exact bounded proposals for custom machine processing. These are intent,
//! never direct inventory writes; accepted plans use the host durable commit.
use super::ComponentValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackValue {
    pub item: String,
    pub count: u16,
    pub components: Option<ComponentValue>,
}

impl StackValue {
    pub fn valid(&self) -> bool {
        !self.item.is_empty()
            && self.item.len() <= 128
            && (1..=128).contains(&self.count)
            && self.components.as_ref().is_none_or(ComponentValue::valid)
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub slot: u8,
    /// Full captured stack, including count and exact component bytes.
    pub expected: StackValue,
    pub count: u16,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub slot: u8,
    pub stack: StackValue,
}

#[derive(Clone, Debug)]
pub struct Transformation {
    /// One to eight distinct owned input slots. At least one item is consumed.
    pub inputs: Vec<Input>,
    /// Zero to eight distinct owned output slots, merged only with exact stacks.
    pub outputs: Vec<Output>,
}

impl Transformation {
    pub fn valid(&self, slots: usize) -> bool {
        let mut inputs = std::collections::BTreeSet::new();
        let mut outputs = std::collections::BTreeSet::new();
        !self.inputs.is_empty()
            && self.inputs.len() <= 8
            && self.outputs.len() <= 8
            && self.inputs.iter().all(|input| {
                usize::from(input.slot) < slots
                    && inputs.insert(input.slot)
                    && input.expected.valid()
                    && (1..=input.expected.count).contains(&input.count)
            })
            && self.outputs.iter().all(|output| {
                usize::from(output.slot) < slots
                    && outputs.insert(output.slot)
                    && output.stack.valid()
            })
    }
}
