//! Active anchored inventories. Hooks declare work; only host operations mutate
//! items. Recipes are explicit item-production authority, transfers conserve exact
//! stacks, and opaque hook data cannot override either inventory.
use crate::{FootprintCell, RegistrationError};
use std::sync::Arc;
mod components;
pub use components::{ComponentMatch, ComponentOutput, ComponentValue};
mod lifecycle;
pub use lifecycle::LifecyclePlan;
#[cfg(test)]
mod tests;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Namespaced allowlist (or `#namespace:item_tag`); empty accepts any item.
    /// Tags expand at startup; missing/empty tags and expansions over 4096 fail.
    pub items: Vec<String>,
    /// Allow exact component-bearing stacks, including registered process inputs.
    pub components: bool,
}
impl Filter {
    pub fn any() -> Self {
        Self {
            items: vec![],
            components: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    /// Outward cardinal normals in world space.
    pub faces: Vec<[i32; 3]>,
    pub insert: Vec<u8>,
    pub extract: Vec<u8>,
}
pub const FACES: [[i32; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];
#[derive(Clone, Debug)]
pub struct Recipe {
    pub key: String,
    pub input: String,
    pub input_count: u16,
    pub input_components: ComponentMatch,
    pub output: String,
    pub output_count: u16,
    pub output_components: ComponentOutput,
    pub pulses: u16,
}
#[derive(Clone, Debug)]
pub struct Fuel {
    pub item: String,
    pub components: ComponentMatch,
    pub pulses: u16,
}
#[derive(Clone, Debug)]
pub struct Process {
    pub input: u8,
    pub output: u8,
    pub fuel: Option<u8>,
    pub recipes: Vec<Recipe>,
    pub fuels: Vec<Fuel>,
}
#[derive(Clone, Debug)]
pub struct Variant {
    pub placement_state: String,
    pub idle: Vec<FootprintCell>,
    /// States while registered fuel is burning; same offsets as `idle`.
    pub active: Vec<FootprintCell>,
}
#[derive(Clone, Debug)]
pub struct Slot<'a> {
    pub item: &'a str,
    pub count: u16,
    pub has_components: bool,
    /// Snapshot-local equivalence class: equal keys mean equal item and exact
    /// components (count excluded). Not a persistent ID and contains no bytes.
    pub stack_key: u8,
}
#[derive(Clone, Debug, Default)]
pub enum StackSelector {
    #[default]
    Any,
    Item(String),
    /// Match the item and full components in this machine's captured own slot.
    /// An empty reference slot matches nothing. Neither bytes nor hashes escape.
    SameAsSlot(u8),
}
#[derive(Clone, Debug)]
pub struct TransferSelection {
    /// Absolute source/destination inventory indices, still constrained by ports.
    pub source_slot: Option<u8>,
    pub destination_slot: Option<u8>,
    pub stack: StackSelector,
    pub count: u16,
}
impl Default for TransferSelection {
    fn default() -> Self {
        Self {
            source_slot: None,
            destination_slot: None,
            stack: StackSelector::Any,
            count: 1,
        }
    }
}
pub struct Context<'a> {
    /// Exact persisted machine identity, independent of dispatch order.
    pub id: u64,
    pub tick: u64,
    pub due: u64,
    pub slots: &'a [Option<Slot<'a>>],
    pub data: &'a [u8],
    pub fuel: u16,
    pub progress: u16,
}
#[derive(Clone, Debug)]
pub enum Work {
    Process,
    Transfer {
        /// Adjacent peer cell relative to this machine's anchor.
        offset: [i32; 3],
        own_port: String,
        peer_port: Option<String>,
        push: bool,
        selection: TransferSelection,
    },
}
#[derive(Clone, Debug)]
pub struct Plan {
    /// Replacement durable private data, at most 1 KiB. Initially empty.
    pub data: Vec<u8>,
    /// Must be later than Context::due; may be behind current tick for catch-up.
    pub next_tick: u64,
    /// Up to eight ordered alternatives; first process or available transfer wins.
    pub work: Vec<Work>,
}
/// Pure retryable planning on immutable inputs. No I/O or external side effects.
pub trait Behavior: Send + Sync + 'static {
    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError>;
}
#[derive(Clone)]
pub struct Machine {
    pub entity: String,
    pub block: String,
    pub item: String,
    pub schema: u64,
    pub slots: u8,
    pub interval: u32,
    pub read_radius: u8,
    pub reads_neighbours: bool,
    pub variants: Vec<Variant>,
    pub filters: Vec<Filter>,
    pub ports: Vec<Port>,
    pub process: Option<Process>,
    pub behavior: Arc<dyn Behavior>,
}
impl std::fmt::Debug for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Machine")
            .field("entity", &self.entity)
            .finish()
    }
}
impl Machine {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let bad = || RegistrationError("invalid machine declaration".into());
        let n = usize::from(self.slots);
        if n == 0
            || n > 54
            || self.interval == 0
            || self.interval > 60000
            || self.read_radius > 1
            || self.filters.len() != n
            || self.variants.is_empty()
            || self.variants.len() > 16
            || self.ports.len() > 16
        {
            return Err(bad());
        }
        let offsets = self.variants[0]
            .idle
            .iter()
            .map(|c| c.offset)
            .collect::<std::collections::BTreeSet<_>>();
        if offsets.is_empty()
            || offsets.len() > 64
            || !offsets.contains(&[0; 3])
            || offsets.iter().flatten().any(|v| v.unsigned_abs() > 16)
        {
            return Err(bad());
        }
        let mut placement_states = std::collections::BTreeSet::new();
        for v in &self.variants {
            if !placement_states.insert(&v.placement_state) {
                return Err(bad());
            }
            for cells in [&v.idle, &v.active] {
                if cells.len() != offsets.len()
                    || cells
                        .iter()
                        .map(|c| c.offset)
                        .collect::<std::collections::BTreeSet<_>>()
                        != offsets
                {
                    return Err(bad());
                }
            }
            if !v
                .idle
                .iter()
                .any(|c| c.offset == [0; 3] && c.state == v.placement_state)
            {
                return Err(bad());
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for p in &self.ports {
            if p.name.is_empty()
                || p.name.len() > 64
                || !names.insert(&p.name)
                || p.faces.is_empty()
                || p.faces.len() > 6
                || p.faces.iter().any(|f| !FACES.contains(f))
                || p.insert
                    .iter()
                    .chain(&p.extract)
                    .any(|s| usize::from(*s) >= n)
                || p.insert.len() > n
                || p.extract.len() > n
            {
                return Err(bad());
            }
        }
        if self.filters.iter().any(|f| f.items.len() > 4096) {
            return Err(bad());
        }
        if let Some(p) = &self.process {
            if p.input == p.output
                || p.fuel.is_some_and(|f| f == p.input || f == p.output)
                || [Some(p.input), Some(p.output), p.fuel]
                    .into_iter()
                    .flatten()
                    .any(|s| usize::from(s) >= n)
                || p.recipes.is_empty()
                || p.recipes.len() > 4096
                || p.fuels.len() > 4096
                || p.fuel.is_some() == p.fuels.is_empty()
            {
                return Err(bad());
            }
            let mut recipes = std::collections::BTreeSet::new();
            let mut fuels = std::collections::BTreeSet::new();
            if p.recipes.iter().any(|r| {
                r.pulses == 0
                    || r.pulses > 60000
                    || !(1..=128).contains(&r.input_count)
                    || !(1..=128).contains(&r.output_count)
                    || !r.input_components.valid()
                    || !r.output_components.valid()
                    || !recipes.insert(&r.key)
            }) || p.fuels.iter().any(|f| {
                f.pulses == 0
                    || f.pulses > 240
                    || !f.components.valid()
                    || !fuels.insert((&f.item, &f.components))
            }) {
                return Err(bad());
            }
            // Disjoint predicates make progress identity independent of recipe
            // ordering. Registration is bounded to 4096 entries, not tick work.
            for (i, r) in p.recipes.iter().enumerate() {
                if p.recipes[..i].iter().any(|old| {
                    old.input == r.input && old.input_components.overlaps(&r.input_components)
                }) {
                    return Err(bad());
                }
            }
            for (i, f) in p.fuels.iter().enumerate() {
                if p.fuels[..i]
                    .iter()
                    .any(|old| old.item == f.item && old.components.overlaps(&f.components))
                {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
    /// Canonical metadata identity. Behavior semantics additionally use `schema`.
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        // Preserve existing machine identities when the new operations are not
        // used. Component declarations themselves are negotiated save identity.
        let component_aware = self.process.as_ref().is_some_and(|p| {
            p.recipes.iter().any(|r| {
                r.input_components != ComponentMatch::Empty
                    || r.output_components != ComponentOutput::Empty
            }) || p
                .fuels
                .iter()
                .any(|f| f.components != ComponentMatch::Empty)
        });
        let mut b = vec![
            if component_aware { 2 } else { 1 },
            self.slots,
            self.read_radius,
            self.reads_neighbours as u8,
        ];
        b.extend(self.interval.to_le_bytes());
        b.extend(self.schema.to_le_bytes());
        fn text(b: &mut Vec<u8>, s: &str) {
            b.extend((s.len() as u32).to_le_bytes());
            b.extend(s.as_bytes());
        }
        for s in [&self.entity, &self.block, &self.item] {
            text(&mut b, s);
        }
        b.push(self.variants.len() as u8);
        for v in &self.variants {
            text(&mut b, &v.placement_state);
            for cells in [&v.idle, &v.active] {
                b.push(cells.len() as u8);
                for c in cells {
                    for v in c.offset {
                        b.extend(v.to_le_bytes());
                    }
                    text(&mut b, &c.state);
                }
            }
        }
        for f in &self.filters {
            b.push(f.components as u8);
            b.extend((f.items.len() as u32).to_le_bytes());
            for s in &f.items {
                text(&mut b, s);
            }
        }
        b.push(self.ports.len() as u8);
        for p in &self.ports {
            text(&mut b, &p.name);
            b.push(p.faces.len() as u8);
            for f in &p.faces {
                for v in f {
                    b.extend(v.to_le_bytes());
                }
            }
            for slots in [&p.insert, &p.extract] {
                b.push(slots.len() as u8);
                b.extend(slots);
            }
        }
        b.push(self.process.is_some() as u8);
        if let Some(p) = &self.process {
            b.extend([p.input, p.output, p.fuel.unwrap_or(255)]);
            b.extend((p.recipes.len() as u32).to_le_bytes());
            for r in &p.recipes {
                text(&mut b, &r.key);
                text(&mut b, &r.input);
                text(&mut b, &r.output);
                if component_aware {
                    r.input_components.fingerprint(&mut b);
                    r.output_components.fingerprint(&mut b);
                }
                for v in [r.input_count, r.output_count, r.pulses] {
                    b.extend(v.to_le_bytes());
                }
            }
            b.extend((p.fuels.len() as u32).to_le_bytes());
            for f in &p.fuels {
                text(&mut b, &f.item);
                if component_aware {
                    f.components.fingerprint(&mut b);
                }
                b.extend(f.pulses.to_le_bytes());
            }
        }
        b
    }
}
pub struct Processor;
impl Behavior for Processor {
    fn plan(&self, c: &Context<'_>) -> Result<Plan, RegistrationError> {
        Ok(Plan {
            data: c.data.to_vec(),
            next_tick: c
                .due
                .checked_add(20)
                .ok_or_else(|| RegistrationError("tick exhausted".into()))?,
            work: vec![Work::Process],
        })
    }
}
pub struct DownwardFlow;
impl Behavior for DownwardFlow {
    fn plan(&self, c: &Context<'_>) -> Result<Plan, RegistrationError> {
        Ok(Plan {
            data: c.data.to_vec(),
            next_tick: c
                .tick
                .checked_add(20)
                .ok_or_else(|| RegistrationError("tick exhausted".into()))?,
            work: vec![
                Work::Transfer {
                    offset: [0, -1, 0],
                    own_port: "storage".into(),
                    peer_port: None,
                    push: true,
                    selection: TransferSelection::default(),
                },
                Work::Transfer {
                    offset: [0, 1, 0],
                    own_port: "storage".into(),
                    peer_port: None,
                    push: false,
                    selection: TransferSelection::default(),
                },
            ],
        })
    }
}
