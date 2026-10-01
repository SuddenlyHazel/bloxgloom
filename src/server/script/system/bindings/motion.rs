//! Owner-local motion services use the same vector parser and exact handles as
//! gameplay. Returned spawn references identify an output ordinal, never an ID.
use super::*;
use crate::server::script::{gameplay, handles};
use bloxgloom_host_api::gameplay::{MotionCommand, MovingSpawn};
struct SpawnRef {
    index: usize,
    scope: std::sync::Arc<()>,
}
impl mlua::UserData for SpawnRef {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, value, ()| {
            Ok(format!("owner-spawn-reference:{}", value.index))
        });
    }
}
#[derive(Clone, Copy)]
pub(super) struct Output<'a> {
    pub scope: &'a std::sync::Arc<()>,
    pub spawns: &'a RefCell<Vec<MovingSpawn>>,
    pub commands: &'a RefCell<Vec<MotionCommand>>,
    pub generic_spawns: &'a RefCell<Vec<api::EntitySpawn>>,
}
pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope api::Context<'_>,
    capabilities: Capabilities,
    rejected: &'scope RefCell<Option<&'static str>>,
    output: Output<'scope>,
) -> mlua::Result<()> {
    host.set(
        "spawn_moving_entity",
        scope.create_function(move |lua, (key, options): (Value, Value)| {
            let index = checked(rejected, || {
                if !capabilities.moving_entities {
                    return Err("system has not declared moving entity creation");
                }
                let spawn = gameplay::bindings::motion::parse_spawn(key, options)
                    .map_err(|_| "invalid moving spawn")?;
                if spawn.state.len() > 1024 {
                    return Err("system moving state exceeds 1024 bytes");
                }
                let mut spawns = output.spawns.borrow_mut();
                if spawns.len() + output.generic_spawns.borrow().len() >= 16 {
                    return Err("system spawn limit exceeded (16)");
                }
                let index = spawns.len();
                spawns.push(spawn);
                Ok(index)
            })?;
            lua.create_userdata(SpawnRef {
                index,
                scope: output.scope.clone(),
            })
            .inspect_err(|_| {
                rejected
                    .borrow_mut()
                    .get_or_insert("moving spawn reference creation failed");
            })
        })?,
    )?;
    host.set(
        "configure_spawn",
        scope.create_function(move |_, (reference, key, options): (Value, Value, Value)| {
            checked(rejected, || {
                if !capabilities.moving_entities {
                    return Err("system has not declared moving entity creation");
                }
                let Value::UserData(reference) = reference else {
                    return Err("configure_spawn requires allocation reference");
                };
                let reference = reference
                    .borrow::<SpawnRef>()
                    .map_err(|_| "invalid spawn reference")?;
                if !std::sync::Arc::ptr_eq(&reference.scope, output.scope) {
                    return Err("foreign spawn reference");
                }
                let spawn = gameplay::bindings::motion::parse_spawn(key, options)
                    .map_err(|_| "invalid moving spawn")?;
                if spawn.state.len() > 1024 {
                    return Err("system moving state exceeds 1024 bytes");
                }
                let mut spawns = output.spawns.borrow_mut();
                let old = spawns
                    .get_mut(reference.index)
                    .ok_or("unknown spawn reference")?;
                if old.key != spawn.key {
                    return Err("spawn reference type cannot change");
                }
                *old = spawn;
                Ok(())
            })
        })?,
    )?;
    host.set(
        "motion",
        scope.create_function(move |lua, id: Value| {
            let motion = checked(rejected, || {
                let id = handles::entity_value(id)?;
                context.motion(id).map_err(|_| "owned motion unavailable")
            })?;
            motion
                .map(|motion| gameplay::events::motion(lua, &motion))
                .transpose()
                .inspect_err(|_| {
                    rejected
                        .borrow_mut()
                        .get_or_insert("motion projection failed");
                })
        })?,
    )?;
    host.set(
        "set_motion",
        scope.create_function(move |_, (id, revision, options): (Value, Value, Value)| {
            checked(rejected, || {
                if !capabilities.motion {
                    return Err("system has not declared motion mutation");
                }
                let id = handles::entity_value(id)?;
                let expected_revision = handles::revision_value(revision)?;
                let Some(motion) = context.motion(id).map_err(|_| "owned motion unavailable")?
                else {
                    return Ok(false);
                };
                if motion.revision != expected_revision {
                    return Err("motion revision mismatch");
                }
                let change = gameplay::bindings::motion::parse_change(options)
                    .map_err(|_| "invalid motion change")?;
                let mut commands = output.commands.borrow_mut();
                if commands.len() >= 16 || commands.iter().any(|c| c.id == id) {
                    return Err("duplicate or excessive motion command");
                }
                commands.push(MotionCommand {
                    id,
                    expected_revision,
                    change,
                });
                Ok(true)
            })
        })?,
    )?;
    Ok(())
}
