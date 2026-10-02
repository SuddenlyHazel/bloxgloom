//! Readonly post-commit Luau callbacks on the native advisory worker lane.
use super::{Invocation, Limits, Program, package::PackageSnapshot, startup::Pending};
use bloxgloom_host_api::gameplay::{Committed, Observer, ObserverRegistration, WeatherChanged};
use mlua::{Function, Lua, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
mod events;

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
    weather: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, module): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_actions(&namespace) {
                return Err("observer registration requires bloxgloom:actions/v1");
            }
            if pending.observers.len() >= 8 {
                return Err("at most eight observers per package");
            }
            let key = super::values::text(key)?;
            if key.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::package::manifest::identifier(local)
            }) {
                return Err("observer must belong to its startup package");
            }
            if pending.observers.iter().any(|r| r.key == key) {
                return Err("duplicate observer");
            }
            let module = super::values::text(module)?;
            if module.split_once(':').map(|p| p.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("observer requires an own-package module");
            }
            let registration = ObserverRegistration {
                key,
                version: {
                    let revision = super::values::integer(revision, 1, u16::MAX.into())? as u16;
                    if weather {
                        snapshot.weather_observer_version(&module, revision)
                    } else {
                        snapshot.gameplay_version(&module, revision)
                    }
                },
                observer: Arc::new(ScriptObserver {
                    weather,
                    snapshot: Arc::clone(&snapshot),
                    module,
                    lifetime: Arc::new(()),
                    realm: NEXT_REALM.fetch_add(1, Ordering::Relaxed),
                }),
            };
            registration.validate().map_err(|_| "invalid observer")?;
            pending.observers.push(registration);
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

struct ScriptObserver {
    weather: bool,
    snapshot: Arc<PackageSnapshot>,
    module: String,
    lifetime: Arc<()>,
    realm: u64,
}
static NEXT_REALM: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static REALMS: RefCell<BTreeMap<u64, (Weak<()>, super::runtime::Retained)>> = const { RefCell::new(BTreeMap::new()) };
}
impl Observer for ScriptObserver {
    fn on_commit(&self, event: &Committed) {
        if self.weather {
            return;
        }
        if let Err(error) = self.invoke(event) {
            tracing::warn!(module = self.module, %error, "committed observer failed");
        }
    }
    fn on_weather(&self, event: &WeatherChanged) {
        if !self.weather {
            return;
        }
        let seed = event.current.elapsed_ms ^ event.current.revision.rotate_left(17);
        if let Err(error) = self.run("WeatherChanged", seed, |lua| {
            let value = lua.create_table()?;
            value.raw_set("kind", "WeatherChanged")?;
            value.raw_set(
                "previous",
                crate::weather::luau::present(lua, event.previous)?,
            )?;
            value.raw_set(
                "current",
                crate::weather::luau::present(lua, event.current)?,
            )?;
            value.set_readonly(true);
            Ok(value)
        }) {
            tracing::warn!(module = self.module, %error, "weather observer failed");
        }
    }
}
impl ScriptObserver {
    fn invoke(&self, event: &Committed) -> Result<(), super::ScriptError> {
        // The world-owned advisory lane serializes these realms. Lifetime tokens
        // prevent a new installation from inheriting the departed world's exports.
        self.run("Committed", events::seed(event), |lua| {
            events::present(lua, event)
        })
    }
    fn run(
        &self,
        kind: &'static str,
        seed: u64,
        present: impl FnOnce(&Lua) -> mlua::Result<mlua::Table>,
    ) -> Result<(), super::ScriptError> {
        REALMS.with(|realms| {
            let mut realms = realms.borrow_mut();
            realms.retain(|_, (owner, _)| owner.strong_count() > 0);
            if !realms.contains_key(&self.realm) && realms.len() >= 32 {
                return Err(super::ScriptError {
                    module: self.module.clone(),
                    failure: super::ScriptFailure::Package(
                        "observer realm admission limit exceeded".into(),
                    ),
                });
            }
            let (_, runtime) = realms.entry(self.realm).or_insert_with(|| {
                (
                    Arc::downgrade(&self.lifetime),
                    super::runtime::Retained::default(),
                )
            });
            runtime.run(
                &Program::Package {
                    snapshot: Arc::clone(&self.snapshot),
                    entry: self.module.clone(),
                    invocation: Invocation::Integer,
                },
                Limits::default(),
                super::runtime::Execution::new(kind, seed, "advisory"),
                |lua, entry| entry.call::<()>(present(lua)?),
            )
        })
    }
}

#[cfg(test)]
mod tests;
