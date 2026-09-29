//! Explicit local `bloxgloom:owner_systems/v1` binding. One system/package:
//! `h.register_system { key='demo:clock', schema=1, revision=1,
//! module='demo:clock', max_state_bytes=64, max_jobs_per_tick=2,
//! read_world=true, read_radius_chunks=1, seeds={{x=0,y=5,z=0,data=''}} }`.
//! Optional `after={'other:system'}` is a bounded, startup-resolved phase edge.
//! Optional `edit_cause='burn'` opts world-read systems into host-owned burn
//! removal semantics; it does not install the deferred native fire owner.
//! Optional `creates_drops=true` grants bounded, host-validated item creation
//! through `c.spawn_drop(x,y,z,item,count,pickup_delay_ms)` at cell centers.
//! Optional `creates_entities=true` permits `c.spawn_entity(key,x,y,z,state)`
//! for registered general entities owned by this package.
//! Optional `reads_entities=true` captures at most 128 package-owned mobile
//! entities in the declared neighborhood as immutable `c.entities` records.
//! Optional `mutates_entities=true` permits conditional state updates/removals
//! of those captured records. It requires `reads_entities=true`.
//!
//! Limits: 32 seeds, 4096 state bytes, 8 jobs/tick, at most 27 chunks/job.
//! The module returns `function(c)` returning `(binary_state, delay_ticks)`;
//! delay is 1..u32::MAX and becomes an absolute durable deadline. Context has
//! readonly owner, owner_kind, data, tick_lo/hi and revision_lo/hi. Chunk owners
//! retain owner[1..3] coordinates. Entity/profile owners use two/four exact u32
//! words respectively, declared as seed `id={...}` with `partition`.
//! Dot methods: block(x,y,z) -> state key (64 calls),
//! edit(x,y,z,before,after) (16 conditional edits), wake(system,x,y,z) (32).
//! Reads see the captured preimage, not earlier proposed edits. Reads/edits
//! require read_world=true; read_radius_chunks defaults to 0 and accepts only
//! 0 or 1. Out-of-neighborhood reads/edits fail, never procedural fallback.
//! Shared removal/placement/neighbor planning can generate entity/drop effects;
//! those effects participate in the same owner WAL record as bytes and edits.
//! Direct drops require a captured world cell and share that receipt as well.
//!
//! Frozen sources execute directly on existing owner workers, in a fresh bounded
//! VM per plan. The public schema fingerprints script schema/revision, module and
//! the entire installation. Any changed identity fails restart, without migration.
//! Opaque bytes have only length validation; script-specific decoding belongs to
//! the planner. Wakes target registered systems through partition-specific
//! methods; absent owners retain a
//! bounded pending flag, not an owner creation request.
//! Script faults fail the existing wave closed (not a silently disabled system).
//!
//! Optional `accepts_intents=true` requires `read_world=true`. It exposes a
//! readonly `c.inbox` (at most 8 deliveries) and `c.send(x,y,z,payload)` (8 sends,
//! 512 bytes each), targeting only this registered system. Each delivery has
//! readonly `id={source={x,y,z},revision_lo,revision_hi,ordinal}`, exact
//! `produced_tick_lo/hi`, and binary `payload`. Successful plans acknowledge the
//! entire inbox in the same WAL record as bytes, edits, wakes, and outgoing mail.
//! Caught send errors poison the plan. Optional `intent_bootstrap='bytes'` is a
//! constant initial state for absent destinations, validated and fingerprinted
//! by the public contract; it grants neither client nor foreign-system authority.
mod bindings;
mod owners;

use super::values::{integer, text};
use super::{Invocation, Limits, Program, package::PackageSnapshot, startup::Pending};
use bloxgloom_host_api::{RegistrationError, system as api};
use mlua::{Function, Lua, Table, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, declaration: Value| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_systems(&namespace) {
                return Err("register_system requires bloxgloom:owner_systems/v1");
            }
            if pending.system.is_some() {
                return Err("only one system per package is allowed");
            }
            let table = table(declaration)?;
            let key = text(field(&table, "key")?)?;
            let Some((owner, local)) = key.split_once(':') else {
                return Err("system key must be namespaced");
            };
            if owner != namespace || !super::package::manifest::identifier(local) {
                return Err("system key must belong to the startup package");
            }
            let module = text(field(&table, "module")?)?;
            if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("system must name a declared module in its package");
            }
            let schema = integer(field(&table, "schema")?, 1, u32::MAX.into())? as u32;
            let revision = integer(field(&table, "revision")?, 1, u16::MAX.into())? as u16;
            let max_bytes = integer(field(&table, "max_state_bytes")?, 1, 4096)? as usize;
            let jobs = integer(field(&table, "max_jobs_per_tick")?, 1, 8)? as u16;
            let partition = owners::partition(field(&table, "partition")?)?;
            let edit_cause = match field(&table, "edit_cause")? {
                Value::Nil => api::EditCause::WorldEdit,
                Value::String(value) if value.as_bytes().as_ref() == b"world_edit" => {
                    api::EditCause::WorldEdit
                }
                Value::String(value) if value.as_bytes().as_ref() == b"burn" => {
                    api::EditCause::Burn
                }
                _ => return Err("edit_cause must be world_edit or burn"),
            };
            let mut read_radius_chunks = match field(&table, "read_world")? {
                Value::Boolean(true) => Some(0),
                Value::Boolean(false) | Value::Nil => None,
                _ => return Err("read_world must be boolean"),
            };
            let radius = field(&table, "read_radius_chunks")?;
            if !matches!(radius, Value::Nil) {
                if read_radius_chunks.is_none() {
                    return Err("read_radius_chunks requires read_world=true");
                }
                read_radius_chunks = Some(integer(radius, 0, 1)? as u8);
            }
            let accepts_intents = match field(&table, "accepts_intents")? {
                Value::Boolean(value) => value,
                Value::Nil => false,
                _ => return Err("accepts_intents must be boolean"),
            };
            let creates_drops = match field(&table, "creates_drops")? {
                Value::Boolean(value) => value,
                Value::Nil => false,
                _ => return Err("creates_drops must be boolean"),
            };
            let creates_entities = match field(&table, "creates_entities")? {
                Value::Boolean(value) => value,
                Value::Nil => false,
                _ => return Err("creates_entities must be boolean"),
            };
            let reads_entities = match field(&table, "reads_entities")? {
                Value::Boolean(value) => value,
                Value::Nil => false,
                _ => return Err("reads_entities must be boolean"),
            };
            let mutates_entities = match field(&table, "mutates_entities")? {
                Value::Boolean(value) => value,
                Value::Nil => false,
                _ => return Err("mutates_entities must be boolean"),
            };
            if creates_drops && read_radius_chunks.is_none() {
                return Err("creates_drops requires read_world=true");
            }
            if creates_entities && read_radius_chunks.is_none() {
                return Err("creates_entities requires read_world=true");
            }
            if reads_entities && read_radius_chunks.is_none() {
                return Err("reads_entities requires read_world=true");
            }
            if mutates_entities && !reads_entities {
                return Err("mutates_entities requires reads_entities=true");
            }
            let intent_bootstrap = match field(&table, "intent_bootstrap")? {
                Value::Nil => None,
                value => Some(bytes(value, max_bytes)?),
            };
            let after = match field(&table, "after")? {
                Value::Nil => Vec::new(),
                Value::Table(table) => {
                    if table.metatable().is_some() || table.raw_len() > 16 {
                        return Err("invalid system after list");
                    }
                    let mut entries = Vec::new();
                    for (index, pair) in table.pairs::<Value, Value>().enumerate() {
                        if index >= 16 {
                            return Err("system after limit exceeded");
                        }
                        let (position, target_value) =
                            pair.map_err(|_| "invalid system after entry")?;
                        let position = integer(position, 1, 16)? as usize;
                        let target = text(target_value)?;
                        if target.split_once(':').is_none_or(|(owner, local)| {
                            !super::package::manifest::identifier(owner)
                                || !super::package::manifest::identifier(local)
                        }) || target == key
                        {
                            return Err("invalid system after key");
                        }
                        entries.push((position, target));
                    }
                    entries.sort_by_key(|(position, _)| *position);
                    if entries
                        .iter()
                        .enumerate()
                        .any(|(index, (position, _))| *position != index + 1)
                    {
                        return Err("system after must be a dense array");
                    }
                    let mut keys = entries.into_iter().map(|(_, key)| key).collect::<Vec<_>>();
                    keys.sort();
                    if keys.windows(2).any(|pair| pair[0] == pair[1]) {
                        return Err("duplicate system after key");
                    }
                    keys
                }
                _ => return Err("system after must be an array"),
            };
            let seeds = self::table(field(&table, "seeds")?)?;
            let mut values = Vec::new();
            // Inspect at most 33 entries, rejecting rather than silently dropping
            // sparse/map keys. Raw pairs do not invoke script metamethods.
            for pair in seeds.pairs::<Value, Value>().take(33) {
                if values.len() == 32 {
                    return Err("system seed limit exceeded (32)");
                }
                let (index, seed) = pair.map_err(|_| "invalid seed")?;
                let index = integer(index, 1, 32)? as usize;
                let seed = self::table(seed)?;
                values.push((
                    index,
                    api::Seed {
                        owner: owners::seed(partition, &seed)?,
                        data: bytes(field(&seed, "data")?, max_bytes)?,
                    },
                ));
            }
            values.sort_by_key(|(index, _)| *index);
            if values
                .iter()
                .enumerate()
                .any(|(i, (index, _))| i + 1 != *index)
            {
                return Err("system seeds must be a dense sequence");
            }
            let system = api::System {
                key,
                schema: snapshot.system_schema(&module, schema, revision),
                partition,
                max_state_bytes: max_bytes as u32,
                max_jobs_per_tick: jobs,
                read_radius_chunks,
                after,
                seeds: values.into_iter().map(|(_, seed)| seed).collect(),
                behavior: Arc::new(ScriptSystem {
                    snapshot: Arc::clone(&snapshot),
                    module,
                    max_bytes,
                    accepts_intents,
                    intent_bootstrap,
                    edit_cause,
                    creates_drops,
                    creates_entities,
                    reads_entities,
                    mutates_entities,
                }),
            };
            system
                .validate()
                .map_err(|_| "invalid system declaration or seed")?;
            pending.system = Some(system);
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

fn table(value: Value) -> Result<Table, &'static str> {
    match value {
        Value::Table(table) => Ok(table),
        _ => Err("expected table"),
    }
}
fn field(table: &Table, name: &str) -> Result<Value, &'static str> {
    table.raw_get(name).map_err(|_| "invalid field")
}
fn bytes(value: Value, max: usize) -> Result<Vec<u8>, &'static str> {
    let Value::String(value) = value else {
        return Err("expected binary state string");
    };
    if value.as_bytes().len() > max {
        return Err("system state byte limit exceeded");
    }
    Ok(value.as_bytes().to_vec())
}
fn cell(x: Value, y: Value, z: Value) -> Result<[i32; 3], &'static str> {
    let axis = |value| integer(value, i32::MIN.into(), i32::MAX.into()).map(|v| v as i32);
    Ok([axis(x)?, axis(y)?, axis(z)?])
}

struct ScriptSystem {
    snapshot: Arc<PackageSnapshot>,
    module: String,
    max_bytes: usize,
    accepts_intents: bool,
    intent_bootstrap: Option<Vec<u8>>,
    edit_cause: api::EditCause,
    creates_drops: bool,
    creates_entities: bool,
    reads_entities: bool,
    mutates_entities: bool,
}
impl api::Behavior for ScriptSystem {
    fn edit_cause(&self) -> api::EditCause {
        self.edit_cause
    }
    fn creates_drops(&self) -> bool {
        self.creates_drops
    }
    fn creates_entities(&self) -> bool {
        self.creates_entities
    }
    fn reads_entities(&self) -> bool {
        self.reads_entities
    }
    fn mutates_entities(&self) -> bool {
        self.mutates_entities
    }
    fn accepts_intents(&self) -> bool {
        self.accepts_intents
    }
    fn intent_bootstrap(&self) -> Option<&[u8]> {
        self.intent_bootstrap.as_deref()
    }
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() > self.max_bytes {
            return Err(RegistrationError("system state byte limit exceeded".into()));
        }
        Ok(())
    }
    fn plan(&self, context: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        // A caller without an outbox must not silently discard sends.
        self.invoke(context, &[], None)
    }
    fn plan_with_intents(
        &self,
        context: &api::Context<'_>,
        inbox: &[api::IntentDelivery],
        outbox: &mut api::IntentOutbox,
    ) -> Result<api::Plan, RegistrationError> {
        if !self.accepts_intents && !inbox.is_empty() {
            return Err(RegistrationError("system does not accept intents".into()));
        }
        self.invoke(context, inbox, self.accepts_intents.then_some(outbox))
    }
}

impl ScriptSystem {
    fn invoke(
        &self,
        context: &api::Context<'_>,
        inbox: &[api::IntentDelivery],
        outbox: Option<&mut api::IntentOutbox>,
    ) -> Result<api::Plan, RegistrationError> {
        super::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            |lua, entry| {
                bindings::invoke(
                    lua,
                    entry,
                    context,
                    self.max_bytes,
                    bindings::Capabilities {
                        drops: self.creates_drops,
                        entities: self.creates_entities,
                        entity_mutations: self.mutates_entities,
                    },
                    inbox,
                    outbox,
                )
            },
        )
        .map_err(|error| RegistrationError(error.to_string()))
    }
}
