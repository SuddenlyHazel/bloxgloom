//! Compile readiness for all delivered client/shared modules.
use super::{ClientBundle, Instant, Lua, identity};

/// Compile every delivered source without executing it. Dormant view/replica
/// callbacks must not postpone a syntax failure until after ContentReady.
pub(super) fn validate_sources(bundle: &ClientBundle) -> Result<(), String> {
    let _reservation = crate::server::script_runtime::Reservation::acquire(16 * 1024 * 1024)
        .map_err(|error| format!("client module validation: {error}"))?;
    let lua = Lua::new();
    lua.set_memory_limit(16 * 1024 * 1024)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + crate::server::script_capacity::CLIENT_PREPARATION_WALL_TIME;
    for (owner, package) in bundle.packages() {
        for (module, source) in &package.sources {
            let id = identity(bundle, &format!("{owner}:{module}"));
            if Instant::now() >= deadline {
                return Err(format!(
                    "client module validation {id}: preparation time limit exceeded"
                ));
            }
            lua.load(&source.source)
                .set_name(&id)
                .into_function()
                .map_err(|error| format!("client module validation {id}: {error}"))?;
            lua.gc_collect()
                .map_err(|error| format!("client module validation {id}: {error}"))?;
            if Instant::now() >= deadline {
                return Err(format!(
                    "client module validation {id}: preparation time limit exceeded"
                ));
            }
        }
    }
    Ok(())
}
