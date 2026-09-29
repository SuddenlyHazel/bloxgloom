//! Conditional direct entity changes from public chunk owners. Every proposal
//! is checked again on the coordinator before joining the shared WAL action.
use super::*;
use crate::server::entities::{
    EntityId, EntityLocation, EntityPatch, EntityPayload, PreparedEntityTransaction,
};

pub(super) fn plan_changes(
    store: &EntityStore,
    catalog: &crate::content::Catalog,
    system_key: &str,
    radius: Option<u8>,
    patches: &[OwnerPatch],
    reads: &mut TerrainReads,
) -> io::Result<Vec<PreparedEntityTransaction>> {
    let mut prepared = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let namespace = system_key.split_once(':').map_or("", |(owner, _)| owner);
    for patch in patches {
        let changes = OwnerEffectPatch::entity_changes(patch);
        if changes.is_empty() {
            continue;
        }
        let owner = patch.owner().as_chunk().ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                "entity change requires chunk owner",
            )
        })?;
        let radius = radius.ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                "entity change requires world capture",
            )
        })?;
        for change in changes {
            if prepared.len() >= 256 || !seen.insert(change.id()) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "duplicate or excessive owner entity change",
                ));
            }
            let id = EntityId::new(change.id()).ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidInput, "invalid owner entity ID")
            })?;
            reads.entities(store.capture_entity_dependency(id))?;
            let snapshot = store
                .snapshot(id)
                .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, "owner entity disappeared"))?;
            let EntityLocation::Mobile { position } = &snapshot.location else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner entity is not mobile",
                ));
            };
            let [x, y, z] = (*position).map(|value| value.floor() as i32);
            let key = crate::world::world_to_chunk(x, y, z).0;
            if !within_radius(key, owner, radius) || snapshot.revision != change.before_revision() {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "owner entity moved or changed",
                ));
            }
            let entity_key = catalog
                .entity_type(snapshot.entity_type)
                .ok_or_else(|| io::Error::other("unknown owner entity type"))?
                .key
                .as_ref();
            if entity_key.split_once(':').map_or("", |(owner, _)| owner) != namespace {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "owner entity belongs to another package",
                ));
            }
            let definition = catalog.gameplay_entity(entity_key).ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner entity has no general schema",
                )
            })?;
            let operation = match change {
                bloxgloom_host_api::system::EntityChange::Update { state, .. } => {
                    if state.len() > 1024
                        || state.len() > usize::from(definition.max_state_bytes)
                        || definition.state.validate(state).is_err()
                        || !definition
                            .state
                            .public(state)
                            .is_ok_and(|view| view.len() <= 4096)
                        || snapshot.private_payload.downcast_ref::<Vec<u8>>() == Some(state)
                    {
                        return Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            "invalid owner entity state",
                        ));
                    }
                    store.prepare_update(
                        id,
                        snapshot.revision,
                        EntityPatch {
                            payload: Some(EntityPayload::new(state.clone())),
                            ..EntityPatch::default()
                        },
                    )
                }
                bloxgloom_host_api::system::EntityChange::Remove { .. } => {
                    store.prepare_despawn(id, snapshot.revision)
                }
            };
            prepared.push(operation.map_err(io::Error::other)?);
        }
    }
    Ok(prepared)
}
