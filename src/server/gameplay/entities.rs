use super::super::durable::TerrainReads;
use super::super::entities::{CellCoord, EntityId, EntityLocation, EntityStore};
use bloxgloom_host_api::gameplay::{Cell, Entity, Error};

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
