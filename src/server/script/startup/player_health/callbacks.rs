//! Fresh-VM immutable damage policies and receipt-bound transition hooks.
use crate::server::script::{self, Invocation, Limits, Program, package::PackageSnapshot, values};
use bloxgloom_host_api::{
    gameplay::{Context, Error},
    player_health::{Damage, DamagePolicy, Event, Hook},
};
use mlua::Value;
use std::{cell::RefCell, sync::Arc};
pub(super) struct ScriptCallback {
    pub(super) snapshot: Arc<PackageSnapshot>,
    pub(super) module: String,
}
impl DamagePolicy for ScriptCallback {
    fn decide(&self, damage: &Damage) -> Result<u32, String> {
        script::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            script::runtime::Execution::new(
                "PlayerDamage",
                0,
                format!(
                    "health:{:032x}:{}",
                    damage.target.profile, damage.health.life
                ),
            ),
            |lua, entry| {
                let input = lua.create_table()?;
                input.set(
                    "player",
                    script::gameplay::bindings::players::present(lua, &damage.target)?,
                )?;
                input.set(
                    "health",
                    script::gameplay::bindings::player_health::present(lua, damage.health)?,
                )?;
                input.set("amount", damage.amount)?;
                input.set("cause", damage.cause.as_str())?;
                input.set_readonly(true);
                let value = entry.call::<Value>(input)?;
                values::integer(
                    value,
                    0,
                    i64::from(bloxgloom_host_api::player_health::MAX_HEALTH),
                )
                .map(|v| v as u32)
                .map_err(|e| mlua::Error::RuntimeError(e.into()))
            },
        )
        .map_err(|e| e.to_string())
    }
}
impl Hook for ScriptCallback {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let rejected = RefCell::new(None);
        let seed = script::runtime::Seed::new()
            .word(context.random_stream_seed())
            .bytes(&event.player.profile.to_le_bytes())
            .word(event.health.life)
            .bytes(event.kind.name().as_bytes())
            .finish();
        let result = script::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            script::runtime::Execution::new(
                event.kind.name(),
                seed,
                format!("health:{:032x}:{}", event.player.profile, event.health.life),
            ),
            |lua, entry| {
                let fields = lua.create_table()?;
                fields.set("kind", event.kind.name())?;
                fields.set(
                    "player",
                    script::gameplay::bindings::players::present(lua, &event.player)?,
                )?;
                fields.set(
                    "before",
                    script::gameplay::bindings::player_health::present(lua, event.before)?,
                )?;
                fields.set(
                    "health",
                    script::gameplay::bindings::player_health::present(lua, event.health)?,
                )?;
                fields.set("cause", event.cause.as_deref())?;
                fields.set_readonly(true);
                let value = script::gameplay::bindings::invoke_fields::<Value>(
                    lua, entry, context, fields, &rejected,
                )?;
                if !value.is_nil() {
                    return Err(mlua::Error::RuntimeError(
                        "health hook must return nil".into(),
                    ));
                }
                Ok(())
            },
        );
        if let Some(error) = rejected.into_inner() {
            return Err(error);
        }
        result.map_err(|e| Error::Invalid(e.to_string()))
    }
}
