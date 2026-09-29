//! Immutable, nominal Luau values for exact host identities and counters.
//! These are values, not authority grants: callers still use the host's normal
//! ownership, captured revision and transaction checks. Weak interning makes an
//! identity usable as a Luau table key without retaining every observed entity.
use mlua::{AnyUserData, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct EntityId(pub u64);
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Revision(pub u64);
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tick(pub u64);
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfileId(pub u128);

macro_rules! display_token {
    ($ty:ident, $prefix:literal, $width:literal) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, ":{:0", $width, "x}"), self.0)
            }
        }
    };
}
display_token!(EntityId, "entity", "16");
display_token!(Revision, "revision", "16");
display_token!(Tick, "tick", "16");
display_token!(ProfileId, "profile", "32");

fn identity_methods<T: UserData + Copy + Eq + fmt::Display + 'static>(
    methods: &mut impl UserDataMethods<T>,
) {
    methods.add_meta_method(MetaMethod::ToString, |_, value, ()| Ok(value.to_string()));
    methods.add_meta_method(MetaMethod::Eq, |_, value, other: AnyUserData| {
        Ok(other.borrow::<T>().is_ok_and(|other| *value == *other))
    });
}
impl UserData for EntityId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        identity_methods(methods);
    }
}
impl UserData for ProfileId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        identity_methods(methods);
    }
}
impl UserData for Revision {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        identity_methods(methods);
        methods.add_method("is_initial", |_, value, ()| Ok(value.0 == 0));
    }
}
impl UserData for Tick {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        identity_methods(methods);
        methods.add_method("before", |_, value, other: AnyUserData| {
            Ok(value.0 < other.borrow::<Tick>()?.0)
        });
        methods.add_method("elapsed_since", |_, value, earlier: AnyUserData| {
            let delta = value.0.checked_sub(earlier.borrow::<Tick>()?.0);
            match delta {
                Some(delta) if delta <= (1u64 << 53) => Ok(delta as f64),
                _ => Err(mlua::Error::RuntimeError(
                    "tick interval is reversed or exceeds exact numeric range".into(),
                )),
            }
        });
    }
}

fn intern<T: UserData + fmt::Display + 'static>(lua: &Lua, value: T) -> mlua::Result<AnyUserData> {
    let cache = match lua.named_registry_value::<Option<Table>>("bloxgloom.handles")? {
        Some(cache) => cache,
        None => {
            let cache = lua.create_table()?;
            let metatable = lua.create_table()?;
            metatable.raw_set("__mode", "v")?;
            cache.set_metatable(Some(metatable))?;
            lua.set_named_registry_value("bloxgloom.handles", cache.clone())?;
            cache
        }
    };
    let key = value.to_string();
    if let Some(handle) = cache.raw_get::<Option<AnyUserData>>(key.as_str())? {
        return Ok(handle);
    }
    let handle = lua.create_userdata(value)?;
    cache.raw_set(key, handle.clone())?;
    Ok(handle)
}

pub(crate) fn entity(lua: &Lua, id: u64) -> mlua::Result<AnyUserData> {
    if id == 0 {
        return Err(mlua::Error::RuntimeError(
            "entity ID must be nonzero".into(),
        ));
    }
    intern(lua, EntityId(id))
}
pub(crate) fn revision(lua: &Lua, value: u64) -> mlua::Result<AnyUserData> {
    intern(lua, Revision(value))
}
pub(crate) fn tick(lua: &Lua, value: u64) -> mlua::Result<AnyUserData> {
    intern(lua, Tick(value))
}
pub(crate) fn profile(lua: &Lua, value: u128) -> mlua::Result<AnyUserData> {
    intern(lua, ProfileId(value))
}
pub(crate) fn entity_value(value: Value) -> Result<u64, &'static str> {
    let Value::UserData(value) = value else {
        return Err("expected entity ID handle");
    };
    value
        .borrow::<EntityId>()
        .map(|v| v.0)
        .map_err(|_| "expected entity ID handle")
}
pub(crate) fn revision_value(value: Value) -> Result<u64, &'static str> {
    let Value::UserData(value) = value else {
        return Err("expected revision token");
    };
    value
        .borrow::<Revision>()
        .map(|v| v.0)
        .map_err(|_| "expected revision token")
}
pub(crate) fn profile_value(value: Value) -> Result<u128, &'static str> {
    let Value::UserData(value) = value else {
        return Err("expected profile ID handle");
    };
    value
        .borrow::<ProfileId>()
        .map(|v| v.0)
        .map_err(|_| "expected profile ID handle")
}

/// A deterministic discrete sample in [0, 1), with no 64-bit float round trip.
pub(super) fn unit_random(value: u64) -> f64 {
    (value >> 11) as f64 / ((1u64 << 53) as f64)
}

#[cfg(test)]
mod tests;
