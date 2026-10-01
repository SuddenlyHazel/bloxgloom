//! Worker-side validation of conditional owner entity proposals. The captured
//! view is the only source of private state and authority for these writes.
use super::*;

pub(super) fn validate_changes(
    changes: &[api::EntityChange],
    context: &api::Context<'_>,
    catalog: Option<&Catalog>,
    permitted: bool,
) -> Result<(), SystemHandlerError> {
    if changes.len() > 16 || (!changes.is_empty() && !permitted) {
        return Err(reject(
            "owner entity mutation requires declared authority and 16-change bound",
        ));
    }
    if changes.is_empty() {
        return Ok(());
    }
    let offered = context
        .entities()
        .ok_or_else(|| reject("owner entity capture missing"))?;
    let catalog = catalog.ok_or_else(|| reject("owner entity catalog missing"))?;
    let mut seen = std::collections::BTreeSet::new();
    for change in changes {
        if !seen.insert(change.id()) {
            return Err(reject("duplicate owner entity mutation"));
        }
        let entity = offered
            .binary_search_by_key(&change.id(), |entity| entity.id)
            .ok()
            .and_then(|index| offered.get(index))
            .ok_or_else(|| reject("owner entity was not captured"))?;
        if entity.revision != change.before_revision() {
            return Err(reject("owner entity revision differs from capture"));
        }
        if let api::EntityChange::Update { state, .. } = change {
            let definition = catalog
                .gameplay_entity(&entity.key)
                .ok_or_else(|| reject("owner entity has no general schema"))?;
            let moving = catalog
                .entity_type_id_by_key(&entity.key)
                .and_then(|id| catalog.moving_entity(id));
            let valid = if let Some(moving) = moving {
                state.len() <= usize::from(moving.max_state_bytes)
                    && moving.state.validate(state).is_ok()
                    && moving
                        .state
                        .public(state)
                        .is_ok_and(|view| view.len() <= usize::from(moving.max_public_bytes))
            } else {
                state.len() <= usize::from(definition.max_state_bytes)
                    && definition.state.validate(state).is_ok()
                    && definition
                        .state
                        .public(state)
                        .is_ok_and(|view| view.len() <= 4096)
            };
            if state == &entity.state || state.len() > 1024 || !valid {
                return Err(reject("invalid owner entity state update"));
            }
        }
    }
    Ok(())
}

fn reject(message: &str) -> SystemHandlerError {
    SystemHandlerError::Rejected(message.into())
}
