//! Inventory operations preserve exact components and stage all-or-nothing.
use super::{Context, Error};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventoryId {
    Player(u128),
    Entity(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Components {
    pub version: u16,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: String,
    pub count: u16,
    pub components: Option<Components>,
}
impl Stack {
    pub fn new(item: impl Into<String>, count: u16) -> Self {
        Self {
            item: item.into(),
            count,
            components: None,
        }
    }
    fn matches(&self, other: &Self) -> bool {
        self.item == other.item && self.components == other.components
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub stack: Option<Stack>,
    pub insert: bool,
    pub extract: bool,
}

impl Context<'_> {
    fn load_inventory(&mut self, owner: InventoryId) -> Result<(), Error> {
        if self.inventories.contains_key(&owner) {
            return Ok(());
        }
        let slots = match self.snapshot.inventory(owner) {
            Ok(slots) => slots,
            Err(error) => return self.fail(error),
        };
        if slots.len() > 256
            || slots
                .iter()
                .filter_map(|slot| slot.stack.as_ref())
                .any(|stack| !(1..=128).contains(&stack.count))
        {
            return self.fail(Error::Host("invalid inventory snapshot".into()));
        }
        self.inventories.insert(owner, slots);
        Ok(())
    }

    pub fn inventory(&mut self, owner: InventoryId) -> Result<Vec<Slot>, Error> {
        self.charge()?;
        self.load_inventory(owner)?;
        Ok(self.inventories[&owner].clone())
    }

    /// Explicit item creation. False means insufficient capacity, with no
    /// partial insertion. Stack identity includes the exact component payload.
    pub fn give(&mut self, owner: InventoryId, stack: Stack) -> Result<bool, Error> {
        self.charge()?;
        if let Err(error) = self.snapshot.validate_stack(&stack) {
            return self.fail(error);
        }
        if !(1..=128).contains(&stack.count) {
            return self.fail(Error::Invalid("invalid stack count".into()));
        }
        self.load_inventory(owner)?;
        let mut slots = self.inventories[&owner].clone();
        let mut remaining = stack.count;
        for (index, slot) in slots.iter_mut().enumerate().filter(|(_, slot)| slot.insert) {
            if !self.snapshot.inventory_accepts(owner, index, &stack) {
                continue;
            }
            if let Some(existing) = &mut slot.stack
                && existing.matches(&stack)
            {
                let added = remaining.min(128 - existing.count);
                existing.count += added;
                remaining -= added;
            }
        }
        for (index, slot) in slots.iter_mut().enumerate().filter(|(_, slot)| slot.insert) {
            if !self.snapshot.inventory_accepts(owner, index, &stack) {
                continue;
            }
            if remaining != 0 && slot.stack.is_none() {
                let mut inserted = stack.clone();
                inserted.count = remaining;
                slot.stack = Some(inserted);
                remaining = 0;
            }
        }
        if remaining != 0 {
            return Ok(false);
        }
        self.store_inventory(owner, slots);
        Ok(true)
    }

    /// Explicit consumption from a slot. None leaves the inventory unchanged.
    pub fn take(
        &mut self,
        owner: InventoryId,
        slot: usize,
        count: u16,
    ) -> Result<Option<Stack>, Error> {
        self.charge()?;
        if !(1..=128).contains(&count) {
            return self.fail(Error::Invalid("invalid take count".into()));
        }
        self.load_inventory(owner)?;
        let mut slots = self.inventories[&owner].clone();
        let Some(source) = slots.get_mut(slot) else {
            return self.fail(Error::Invalid("inventory slot out of range".into()));
        };
        let Some(stack) = &mut source.stack else {
            return Ok(None);
        };
        if !source.extract || stack.count < count {
            return Ok(None);
        }
        let mut taken = stack.clone();
        taken.count = count;
        stack.count -= count;
        if stack.count == 0 {
            source.stack = None;
        }
        self.store_inventory(owner, slots);
        Ok(Some(taken))
    }

    /// An exact transfer, never a take followed by a best-effort give. Failure
    /// preserves both inventories, including for same-inventory slot moves.
    pub fn transfer(
        &mut self,
        from: InventoryId,
        source: usize,
        to: InventoryId,
        destination: usize,
        count: u16,
    ) -> Result<bool, Error> {
        self.charge()?;
        if !(1..=128).contains(&count) {
            return self.fail(Error::Invalid("invalid transfer count".into()));
        }
        self.load_inventory(from)?;
        self.load_inventory(to)?;
        let (Some(source_slot), Some(destination_slot)) = (
            self.inventories[&from].get(source),
            self.inventories[&to].get(destination),
        ) else {
            return self.fail(Error::Invalid("inventory slot out of range".into()));
        };
        let Some(stack) = &source_slot.stack else {
            return Ok(false);
        };
        if !source_slot.extract || !destination_slot.insert || stack.count < count {
            return Ok(false);
        }
        if from == to && source == destination {
            return Ok(true);
        }
        if !self.snapshot.inventory_accepts(to, destination, stack) {
            return Ok(false);
        }
        if destination_slot
            .stack
            .as_ref()
            .is_some_and(|other| !stack.matches(other) || other.count + count > 128)
        {
            return Ok(false);
        }
        let mut moved = stack.clone();
        moved.count = count;
        let mut source_slots = self.inventories[&from].clone();
        let source_stack = source_slots[source].stack.as_mut().unwrap();
        source_stack.count -= count;
        if source_stack.count == 0 {
            source_slots[source].stack = None;
        }
        let mut destination_slots = if from == to {
            source_slots.clone()
        } else {
            self.inventories[&to].clone()
        };
        if let Some(stack) = &mut destination_slots[destination].stack {
            stack.count += count;
        } else {
            destination_slots[destination].stack = Some(moved);
        }
        if from != to {
            self.store_inventory(from, source_slots);
        }
        self.store_inventory(to, destination_slots);
        Ok(true)
    }

    fn store_inventory(&mut self, owner: InventoryId, slots: Vec<Slot>) {
        self.plan
            .inventories
            .insert(owner, slots.iter().map(|slot| slot.stack.clone()).collect());
        self.inventories.insert(owner, slots);
    }
}
