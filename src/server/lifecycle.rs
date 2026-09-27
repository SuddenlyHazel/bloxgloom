//! Public declarations resolved into frozen host services before recovery.
use crate::content::{BlockStateId, BlockTypeId, Catalog, EntityTypeId};
use crate::items::ItemId;
use bloxgloom_host_api::{CubeBlock, Extension, Registrar, RegistrationError, StorageBlockEntity};
use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct Registration {
    cubes: Vec<CubeBlock>,
    pub definitions: Vec<StorageBlockEntity>,
    screens: Vec<bloxgloom_host_api::InventoryScreen>,
    mobiles: Vec<bloxgloom_host_api::entity::MobileEntity>,
}
impl Registrar for Registration {
    fn mobile_entity(
        &mut self,
        entity: bloxgloom_host_api::entity::MobileEntity,
    ) -> Result<(), RegistrationError> {
        entity.validate()?;
        if self.mobiles.iter().any(|e| e.key == entity.key) {
            return Err(RegistrationError("duplicate mobile declaration".into()));
        }
        self.mobiles.push(entity);
        Ok(())
    }
    fn inventory_screen(
        &mut self,
        screen: bloxgloom_host_api::InventoryScreen,
    ) -> Result<(), RegistrationError> {
        screen.validate()?;
        if self.screens.iter().any(|s| s.entity == screen.entity) {
            return Err(RegistrationError("duplicate inventory screen".into()));
        }
        self.screens.push(screen);
        Ok(())
    }
    fn cube_block(&mut self, block: CubeBlock) -> Result<(), RegistrationError> {
        if self.cubes.iter().any(|b| b.key == block.key) {
            return Err(RegistrationError("duplicate block declaration".into()));
        }
        self.cubes.push(block);
        Ok(())
    }
    fn storage_block_entity(
        &mut self,
        entity: StorageBlockEntity,
    ) -> Result<(), RegistrationError> {
        entity.validate()?;
        if self
            .definitions
            .iter()
            .any(|d| d.entity == entity.entity || d.block == entity.block)
        {
            return Err(RegistrationError("duplicate lifecycle owner".into()));
        }
        self.definitions.push(entity);
        Ok(())
    }
}
impl Registration {
    pub fn install(
        extension: &dyn Extension,
        catalog: &mut Catalog,
    ) -> Result<Self, RegistrationError> {
        let mut registration = Self::default();
        extension.register(&mut registration)?;
        let mut candidate = catalog.clone();
        for mobile in &registration.mobiles {
            candidate.register_mobile(mobile.clone())?;
        }
        for cube in &registration.cubes {
            candidate.extension_cube(cube)?;
        }
        for definition in &registration.definitions {
            candidate.extension_storage(definition)?;
        }
        for screen in &registration.screens {
            candidate.register_inventory_screen(screen.clone())?;
        }
        Registry::resolve(&candidate, &registration.definitions)?;
        candidate
            .storage_lifecycles
            .extend(registration.definitions.clone());
        *catalog = candidate;
        Ok(registration)
    }
}

pub(super) struct Resolved {
    pub definition: StorageBlockEntity,
    pub entity: EntityTypeId,
    pub block: BlockTypeId,
    pub item: ItemId,
    pub anchor: BlockStateId,
    pub states: Vec<BlockStateId>,
}

#[derive(Default)]
pub(super) struct Registry {
    pub entries: BTreeMap<BlockTypeId, Arc<Resolved>>,
}
impl Registry {
    pub fn resolve(
        catalog: &Catalog,
        definitions: &[StorageBlockEntity],
    ) -> Result<Self, RegistrationError> {
        let missing =
            || RegistrationError("unresolved or incompatible storage lifecycle reference".into());
        let mut entries = BTreeMap::new();
        let mut entities = std::collections::BTreeSet::new();
        for definition in definitions {
            definition.validate()?;
            let entity = catalog
                .entity_type_id_by_key(&definition.entity)
                .ok_or_else(missing)?;
            let screen = catalog.inventory_screen(entity).ok_or_else(missing)?;
            let offsets: std::collections::BTreeSet<_> =
                definition.footprint.iter().map(|c| c.offset).collect();
            if usize::from(screen.slots) != definition.slots
                || screen.block != definition.block
                || screen
                    .footprint
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    != offsets
            {
                return Err(RegistrationError(
                    "inventory screen differs from storage lifecycle".into(),
                ));
            }
            let block = catalog
                .block_by_key(&definition.block)
                .ok_or_else(missing)?;
            let anchor = catalog
                .state_by_key(&definition.anchor_state)
                .ok_or_else(missing)?;
            let item = catalog
                .items()
                .find(|i| i.key == definition.placement_item && i.placeable == Some(anchor))
                .ok_or_else(missing)?
                .id;
            let states = definition
                .footprint
                .iter()
                .map(|cell| {
                    catalog
                        .state_by_key(&cell.state)
                        .filter(|s| catalog.state(*s).is_some_and(|s| s.block_type == block))
                        .ok_or_else(missing)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if entries.contains_key(&block) || !entities.insert(entity) {
                return Err(RegistrationError("duplicate lifecycle owner".into()));
            }
            entries.insert(
                block,
                Arc::new(Resolved {
                    definition: definition.clone(),
                    entity,
                    block,
                    item,
                    anchor,
                    states,
                }),
            );
        }
        Ok(Self { entries })
    }
    pub fn for_state(&self, catalog: &Catalog, state: BlockStateId) -> Option<&Arc<Resolved>> {
        self.entries.get(&catalog.state(state)?.block_type)
    }
}
