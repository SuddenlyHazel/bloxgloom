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
    icons: Vec<bloxgloom_host_api::icon::ItemIcon>,
    anchored: Vec<bloxgloom_host_api::anchored::AnchoredBlockEntity>,
    content: crate::content::declarations::Declarations,
    cubes: Vec<CubeBlock>,
    pub definitions: Vec<StorageBlockEntity>,
    screens: Vec<bloxgloom_host_api::InventoryScreen>,
    mobiles: Vec<bloxgloom_host_api::entity::MobileEntity>,
    machines: Vec<bloxgloom_host_api::machine::Machine>,
    actions: Vec<bloxgloom_host_api::actions::Action>,
    systems: Vec<bloxgloom_host_api::system::System>,
}
impl Registrar for Registration {
    fn item_icon(
        &mut self,
        icon: bloxgloom_host_api::icon::ItemIcon,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        icon.validate()?;
        self.icons.push(icon);
        Ok(())
    }
    fn owner_system(
        &mut self,
        system: bloxgloom_host_api::system::System,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        system.validate()?;
        if self.systems.iter().any(|old| old.key == system.key) {
            return Err(RegistrationError("duplicate owner system".into()));
        }
        self.systems.push(system);
        Ok(())
    }
    fn anchored_block_entity(
        &mut self,
        entity: bloxgloom_host_api::anchored::AnchoredBlockEntity,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        entity.validate()?;
        self.anchored.push(entity);
        Ok(())
    }
    fn package(
        &mut self,
        d: bloxgloom_host_api::composition::Package,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        self.content.package(d)
    }
    fn texture(
        &mut self,
        d: bloxgloom_host_api::content::Texture,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        self.content.texture(d)
    }
    fn block(&mut self, d: bloxgloom_host_api::content::Block) -> Result<(), RegistrationError> {
        self.room()?;
        self.content.block(d)
    }
    fn item(&mut self, d: bloxgloom_host_api::content::Item) -> Result<(), RegistrationError> {
        self.room()?;
        self.content.item(d)
    }
    fn tag(&mut self, d: bloxgloom_host_api::content::Tag) -> Result<(), RegistrationError> {
        self.room()?;
        self.content.tag(d)
    }
    fn action(
        &mut self,
        action: bloxgloom_host_api::actions::Action,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        action.validate()?;
        if self.actions.len() >= bloxgloom_host_api::actions::MAX_ACTIONS
            || self.actions.iter().any(|a| a.key == action.key)
        {
            return Err(RegistrationError(
                "duplicate action or action capacity exceeded".into(),
            ));
        }
        self.actions.push(action);
        Ok(())
    }
    fn machine(
        &mut self,
        m: bloxgloom_host_api::machine::Machine,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        m.validate()?;
        if self
            .machines
            .iter()
            .any(|old| old.entity == m.entity || old.block == m.block)
        {
            return Err(RegistrationError("duplicate machine".into()));
        }
        self.machines.push(m);
        Ok(())
    }
    fn mobile_entity(
        &mut self,
        entity: bloxgloom_host_api::entity::MobileEntity,
    ) -> Result<(), RegistrationError> {
        self.room()?;
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
        self.room()?;
        screen.validate()?;
        if self.screens.iter().any(|s| s.entity == screen.entity) {
            return Err(RegistrationError("duplicate inventory screen".into()));
        }
        self.screens.push(screen);
        Ok(())
    }
    fn cube_block(&mut self, block: CubeBlock) -> Result<(), RegistrationError> {
        self.room()?;
        if block.key.len() > 255 || block.name.len() > 255 || block.texture.len() > 255 {
            return Err(RegistrationError("cube declaration too large".into()));
        }
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
        self.room()?;
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
    fn room(&self) -> Result<(), RegistrationError> {
        if self.content.len()
            + self.cubes.len()
            + self.definitions.len()
            + self.screens.len()
            + self.mobiles.len()
            + self.machines.len()
            + self.anchored.len()
            + self.systems.len()
            + self.actions.len()
            + self.icons.len()
            >= 4096
        {
            return Err(RegistrationError(
                "registration exceeds 4096 declarations".into(),
            ));
        }
        Ok(())
    }
    pub fn install(
        extension: &dyn Extension,
        catalog: &mut Catalog,
    ) -> Result<Self, RegistrationError> {
        let mut registration = Self::default();
        extension.register(&mut registration)?;
        let mut candidate = catalog.clone();
        registration.systems.sort_by(|a, b| a.key.cmp(&b.key));
        registration
            .anchored
            .sort_by(|a, b| a.entity.cmp(&b.entity));
        for system in &registration.systems {
            candidate.register_owner_system(system.clone())?;
        }
        registration.content.install_base(&mut candidate)?;
        registration.cubes.sort_by(|a, b| a.key.cmp(&b.key));
        registration.mobiles.sort_by(|a, b| a.key.cmp(&b.key));
        registration
            .definitions
            .sort_by(|a, b| a.entity.cmp(&b.entity));
        registration
            .machines
            .sort_by(|a, b| a.entity.cmp(&b.entity));
        for mobile in &registration.mobiles {
            candidate.register_mobile(mobile.clone())?;
        }
        for cube in &registration.cubes {
            candidate.extension_cube(cube)?;
        }
        registration
            .content
            .install_items_and_tags(&mut candidate)?;
        candidate.refresh_builtin_fuels()?;
        for icon in &registration.icons {
            candidate.register_item_icon(icon.clone())?;
        }
        for definition in &registration.definitions {
            candidate.extension_storage(definition)?;
        }
        for definition in &registration.anchored {
            candidate.register_anchored(definition.clone())?;
        }
        for machine in &registration.machines {
            candidate.register_machine_identity(machine)?;
        }
        for screen in &registration.screens {
            candidate.register_inventory_screen(screen.clone())?;
        }
        for machine in &registration.machines {
            let id = candidate.entity_type_id_by_key(&machine.entity).unwrap();
            candidate.bind_machine(id, Arc::new(machine.clone()))?;
        }
        Registry::resolve(&candidate, &registration.definitions)?;
        for action in &registration.actions {
            candidate.register_action(action.clone())?;
        }
        candidate
            .validate()
            .map_err(|error| RegistrationError(format!("invalid composed catalog: {error:?}")))?;
        candidate
            .storage_lifecycles
            .extend(registration.definitions.clone());
        candidate
            .validate()
            .map_err(|e| RegistrationError(format!("invalid catalog: {e:?}")))?;
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
