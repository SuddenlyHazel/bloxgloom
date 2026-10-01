//! Startup `register_entity(key, schema_version, state_bytes, public_bytes, delay)`.
//! Requires `bloxgloom:actions/v1`. Keys are owned by the declaring package;
//! versions are 1..65535, fixed state sizes 1..65535, public prefix sizes
//! 0..min(state_bytes,4096), and initial delays nil (suspended) or 1..100000.
//! Every binary string of exactly state_bytes is canonical; authors encode their
//! own fields, without coercion, padding or a second serializer. Only the explicit
//! prefix is public. Zero exposes no bytes. No VM executes during codec calls.
//! The schema fingerprint covers these choices and the entire frozen package
//! installation, rejecting changed sources on restart even for suspended types.
use super::{
    package::PackageSnapshot,
    startup::Pending,
    values::{integer, text},
};
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::{EntityDefinition, EntityState},
};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_, (key, schema, size, public, delay): (Value, Value, Value, Value, Value)| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !snapshot.permits_actions(&namespace) {
                    return Err("register_entity requires bloxgloom:actions/v1");
                }
                if pending.entities.len() >= 32 {
                    return Err("gameplay entity limit exceeded (32 per package)");
                }
                let key = text(key)?;
                if key.split_once(':').is_none_or(|(owner, local)| {
                    owner != namespace || !super::package::manifest::identifier(local)
                }) {
                    return Err("entity key must belong to the startup package");
                }
                if pending.entities.iter().any(|old| old.key == key) {
                    return Err("duplicate gameplay entity key");
                }
                let schema_version = integer(schema, 1, u16::MAX.into())? as u16;
                let state_bytes = integer(size, 1, u16::MAX.into())? as u16;
                let public_bytes = integer(public, 0, i64::from(state_bytes.min(4096)))? as u16;
                let initial_delay_ticks = if delay.is_nil() {
                    None
                } else {
                    Some(integer(delay, 1, 100_000)? as u32)
                };
                let definition = EntityDefinition {
                    schema_fingerprint: snapshot.entity_schema(
                        &key,
                        schema_version,
                        state_bytes,
                        public_bytes,
                        initial_delay_ticks,
                    ),
                    key,
                    schema_version,
                    max_state_bytes: state_bytes,
                    initial_delay_ticks,
                    state: Arc::new(FixedBytes {
                        state_bytes,
                        public_bytes,
                    }),
                };
                definition
                    .validate()
                    .map_err(|_| "invalid gameplay entity contract")?;
                pending.entities.push(definition);
                Ok(())
            })();
            result.map_err(|error| {
                // A caught registration error cannot publish partial declarations.
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}

pub(in crate::server::script) struct FixedBytes {
    pub(in crate::server::script) state_bytes: u16,
    pub(in crate::server::script) public_bytes: u16,
}

impl EntityState for FixedBytes {
    fn validate(&self, bytes: &[u8]) -> Result<(), RegistrationError> {
        if bytes.len() != usize::from(self.state_bytes) {
            return Err(RegistrationError(
                "entity state must match declared fixed byte length".into(),
            ));
        }
        Ok(())
    }

    fn public(&self, bytes: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        self.validate(bytes)?;
        // Slice before allocating: projection work and output are both bounded.
        Ok(bytes[..usize::from(self.public_bytes)].to_vec())
    }
}
