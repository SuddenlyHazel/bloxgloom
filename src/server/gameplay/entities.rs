use super::super::durable::TerrainReads;
use super::super::entities::{CellCoord, EntityId, EntityLocation, EntityStore};
use bloxgloom_host_api::gameplay::{Cell, Entity, Error};

pub(super) fn state(
    catalog: &crate::content::Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    id: u64,
    owner: &str,
) -> Result<Option<Vec<u8>>, Error> {
    let id = EntityId::new(id).ok_or_else(|| Error::Invalid("entity ID must be nonzero".into()))?;
    reads
        .entities(store.capture_entity_dependency(id))
        .map_err(|_| Error::BudgetExceeded)?;
    let Some(snapshot) = store.snapshot(id) else {
        return Ok(None);
    };
    let key = catalog
        .entity_type(snapshot.entity_type)
        .ok_or_else(|| Error::Host("unknown entity type".into()))?
        .key
        .as_ref();
    validate_owner(catalog, key, owner)?;
    snapshot
        .private_payload
        .downcast_ref::<Vec<u8>>()
        .cloned()
        .map(Some)
        .ok_or_else(|| Error::Host("invalid gameplay entity payload".into()))
}

fn validate_owner<'a>(
    catalog: &'a crate::content::Catalog,
    key: &str,
    owner: &str,
) -> Result<&'a bloxgloom_host_api::gameplay::EntityDefinition, Error> {
    let definition = catalog
        .gameplay_entity(key)
        .ok_or_else(|| Error::Invalid(format!("{key}: entity has no general gameplay state")))?;
    if key
        .split_once(':')
        .is_none_or(|(namespace, _)| namespace != owner)
    {
        return Err(Error::Invalid(format!(
            "{key}: entity is not owned by {owner}"
        )));
    }
    Ok(definition)
}

pub(super) fn validate_state(
    catalog: &crate::content::Catalog,
    key: &str,
    owner: &str,
    state: &[u8],
) -> Result<(), Error> {
    let definition = validate_owner(catalog, key, owner)?;
    if state.len() > usize::from(definition.max_state_bytes) {
        return Err(Error::Invalid(format!(
            "{key}: entity state exceeds declared bound"
        )));
    }
    definition
        .state
        .validate(state)
        .map_err(|error| Error::Invalid(format!("{key}: {error}")))?;
    let public = definition
        .state
        .public(state)
        .map_err(|error| Error::Invalid(format!("{key}: {error}")))?;
    if public.len() > 4096 {
        return Err(Error::Invalid(format!("{key}: public view exceeds bound")));
    }
    Ok(())
}

pub(super) fn project(store: &EntityStore, id: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
    let id = EntityId::new(id).ok_or_else(|| Error::Invalid("invalid entity ID".into()))?;
    let snapshot = store
        .snapshot(id)
        .ok_or_else(|| Error::Host("entity disappeared during planning".into()))?;
    let descriptor = store
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(|e| Error::Host(e.to_string()))?;
    descriptor
        .public_view(&super::super::entities::EntityPayload::new(state.to_vec()))
        .map_err(|e| Error::Invalid(e.to_string()))
}

pub(super) fn read(
    catalog: &crate::content::Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    id: u64,
) -> Result<Option<Entity>, Error> {
    let id = EntityId::new(id).ok_or_else(|| Error::Invalid("entity ID must be nonzero".into()))?;
    reads
        .entities(store.capture_entity_dependency(id))
        .map_err(|_| Error::BudgetExceeded)?;
    let Some(view) = store.public_view(id) else {
        return Ok(None);
    };
    let definition = catalog
        .entity_type(view.entity_type)
        .ok_or_else(|| Error::Host("unknown stored entity type".into()))?;
    let (position, anchor) = match view.location {
        EntityLocation::Mobile { position } => (position, None),
        EntityLocation::Anchored { anchor, .. } => {
            let cell = [anchor.x, anchor.y, anchor.z];
            (cell.map(|n| n as f32 + 0.5), Some(cell))
        }
    };
    Ok(Some(Entity {
        id: id.get(),
        entity_type: definition.key.to_string(),
        position,
        anchor,
        data: view.payload,
    }))
}

pub(super) fn anchored(
    reads: &mut TerrainReads,
    store: &EntityStore,
    cell: Cell,
) -> Result<Option<u64>, Error> {
    let cell = CellCoord::new(cell[0], cell[1], cell[2]);
    let dependencies = store.capture_anchor_dependency(cell);
    reads
        .entities(dependencies)
        .map_err(|_| Error::BudgetExceeded)?;
    Ok(store.anchored_at(cell).map(EntityId::get))
}

pub(super) fn nearby(
    catalog: &crate::content::Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    position: [f32; 3],
    radius: f32,
) -> Result<Vec<Entity>, Error> {
    let dependencies = store
        .capture_mobile_dependencies(position, radius)
        .map_err(|_| Error::BudgetExceeded)?;
    reads
        .entities(dependencies)
        .map_err(|_| Error::BudgetExceeded)?;
    let min = position.map(|n| n - radius);
    let max = position.map(|n| n + radius);
    let ids = store
        .query_mobile_aabb(min, max)
        .map_err(|_| Error::BudgetExceeded)?;
    let radius_squared = f64::from(radius) * f64::from(radius);
    let mut matches = Vec::new();
    for id in ids {
        let entity = read(catalog, reads, store, id.get())?
            .ok_or_else(|| Error::Host("queried entity disappeared".into()))?;
        let distance_squared = entity
            .position
            .iter()
            .zip(position)
            .map(|(coordinate, center)| {
                let difference = f64::from(*coordinate) - f64::from(center);
                difference * difference
            })
            .sum::<f64>();
        if distance_squared <= radius_squared {
            if matches.len() == 128 {
                return Err(Error::BudgetExceeded);
            }
            matches.push(entity);
        }
    }
    Ok(matches)
}
