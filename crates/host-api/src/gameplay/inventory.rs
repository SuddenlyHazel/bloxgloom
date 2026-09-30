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

/// Routing for one eligible pickup candidate. A failed host transfer leaves
/// `remaining` unchanged so later slots can still receive those items.
pub struct PickupTransfer<'a> {
    source: &'a Stack,
    remaining: u16,
}

impl<'a> PickupTransfer<'a> {
    pub fn new(source: &'a Stack, maximum: u16) -> Self {
        Self {
            source,
            remaining: maximum.min(source.count).min(128),
        }
    }

    pub fn remaining(&self) -> u16 {
        self.remaining
    }

    /// Propose a bounded exact-component transfer into one destination slot.
    /// Host permissions and filters are checked again by `Context::transfer`.
    pub fn offer(&self, slot: &Slot) -> u16 {
        if !slot.insert {
            return 0;
        }
        let capacity = match &slot.stack {
            Some(other) if other.matches(self.source) && other.count <= 128 => 128 - other.count,
            None => 128,
            _ => 0,
        };
        self.remaining.min(capacity)
    }

    /// Call only after an exact host transfer succeeds.
    pub fn credited(&mut self, amount: u16) -> Result<(), Error> {
        if amount == 0 || amount > self.remaining {
            return Err(Error::Invalid("pickup credited more than available".into()));
        }
        self.remaining -= amount;
        Ok(())
    }
}

impl Context<'_> {
    /// Collect from a host-accessible drop into the authenticated player's
    /// finite inventory. Returns the exact credited count (possibly zero).
    /// The host owns candidate range, drop extraction eligibility and commit;
    /// transfer filters can reject a slot without starving later slots.
    pub fn collect_drop(&mut self, id: u64, maximum: u16) -> Result<u16, Error> {
        let player = self
            .player()
            .ok_or_else(|| Error::Invalid("pickup needs a player".into()))?;
        if !self.snapshot.pickup_eligible(id) {
            return Ok(0);
        }
        let source = InventoryId::Entity(id);
        let Some(slot) = self.inventory(source)?.into_iter().next() else {
            return Ok(0);
        };
        let Some(stack) = slot.stack else {
            return Ok(0);
        };
        if !slot.extract {
            return Ok(0);
        }
        let mut routing = PickupTransfer::new(&stack, maximum);
        let requested = routing.remaining();
        for (index, destination) in self.inventory(player)?.iter().enumerate() {
            if routing.remaining() == 0 {
                break;
            }
            let amount = routing.offer(destination);
            if amount != 0 && self.transfer(source, 0, player, index, amount)? {
                routing.credited(amount)?;
            }
        }
        Ok(requested - routing.remaining())
    }

    fn load_inventory(&mut self, owner: InventoryId) -> Result<(), Error> {
        if let Err(error) = self
            .snapshot
            .authorize_inventory(owner, self.handler_namespace.as_deref())
        {
            return self.fail(error);
        }
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

    /// Existing player-style slot move: exact move to empty, partial merge up
    /// to the 128 cap, or full-stack swap with a different item. A rejected
    /// move leaves every slot unchanged. Entity inventory filters still apply.
    pub fn move_slots(
        &mut self,
        owner: InventoryId,
        from: usize,
        to: usize,
        count: u16,
    ) -> Result<bool, Error> {
        self.charge()?;
        if from == to || !(1..=128).contains(&count) {
            return Ok(false);
        }
        self.load_inventory(owner)?;
        let mut slots = self.inventories[&owner].clone();
        let (Some(source), Some(destination)) = (slots.get(from).cloned(), slots.get(to).cloned())
        else {
            return Ok(false);
        };
        let Some(stack) = source.stack else {
            return Ok(false);
        };
        if !source.extract
            || !destination.insert
            || stack.count < count
            || !self.snapshot.inventory_accepts(owner, to, &stack)
        {
            return Ok(false);
        }
        let moved = match destination.stack {
            None => {
                let mut moved = stack.clone();
                moved.count = count;
                slots[to].stack = Some(moved);
                count
            }
            Some(target) if target.matches(&stack) => {
                let moved = count.min(128 - target.count);
                if moved == 0 {
                    return Ok(false);
                }
                slots[to].stack.as_mut().unwrap().count += moved;
                moved
            }
            Some(target)
                if count == stack.count
                    && destination.extract
                    && slots[from].insert
                    && self.snapshot.inventory_accepts(owner, from, &target) =>
            {
                slots[from].stack = Some(target);
                slots[to].stack = Some(stack);
                self.store_inventory(owner, slots);
                return Ok(true);
            }
            Some(_) => return Ok(false),
        };
        let remainder = stack.count - moved;
        slots[from].stack = (remainder != 0).then(|| {
            let mut remaining = stack.clone();
            remaining.count = remainder;
            remaining
        });
        self.store_inventory(owner, slots);
        Ok(true)
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
