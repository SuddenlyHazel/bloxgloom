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
    player_lifecycles: Vec<bloxgloom_host_api::players::Registration>,
    gameplay_observers: Vec<bloxgloom_host_api::gameplay::ObserverRegistration>,
    gameplay_entities: Vec<bloxgloom_host_api::gameplay::EntityDefinition>,
    gameplay_handlers: Vec<bloxgloom_host_api::gameplay::HandlerRegistration>,
    icons: Vec<bloxgloom_host_api::icon::ItemIcon>,
    anchored: Vec<bloxgloom_host_api::anchored::AnchoredBlockEntity>,
    content: crate::content::declarations::Declarations,
    cubes: Vec<CubeBlock>,
    pub definitions: Vec<StorageBlockEntity>,
    screens: Vec<bloxgloom_host_api::InventoryScreen>,
    mobiles: Vec<bloxgloom_host_api::entity::MobileEntity>,
    moving: Vec<bloxgloom_host_api::motion::MovingEntity>,
    machines: Vec<bloxgloom_host_api::machine::Machine>,
    actions: Vec<bloxgloom_host_api::actions::Action>,
    systems: Vec<bloxgloom_host_api::system::System>,
    pub(crate) generation: Vec<bloxgloom_host_api::generation::Registration>,
}
impl Registrar for Registration {
    fn player_lifecycle(
        &mut self,
        registration: bloxgloom_host_api::players::Registration,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        registration.validate()?;
        self.player_lifecycles.push(registration);
        Ok(())
    }

    fn generation_contributor(
        &mut self,
        contributor: bloxgloom_host_api::generation::Registration,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        contributor.validate()?;
        if self.generation.iter().any(|old| old.key == contributor.key) {
            return Err(RegistrationError(format!(
                "{}: duplicate generation contributor",
                contributor.key
            )));
        }
        if self.generation.len() >= 256 {
            return Err(RegistrationError(format!(
                "{} generators/installation: attempted {}; maximum 256",
                contributor.key,
                self.generation.len() + 1
            )));
        }
        self.generation.push(contributor);
        Ok(())
    }
    fn gameplay_observer(
        &mut self,
        observer: bloxgloom_host_api::gameplay::ObserverRegistration,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        observer.validate()?;
        self.gameplay_observers.push(observer);
        Ok(())
    }
    fn gameplay_entity(
        &mut self,
        definition: bloxgloom_host_api::gameplay::EntityDefinition,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        definition.validate()?;
        self.gameplay_entities.push(definition);
        Ok(())
    }
    fn gameplay_handler(
        &mut self,
        handler: bloxgloom_host_api::gameplay::HandlerRegistration,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        handler.validate()?;
        self.gameplay_handlers.push(handler);
        Ok(())
    }
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
            return Err(RegistrationError(format!(
                "duplicate action {} or action capacity exceeded ({} registered)",
                action.key,
                self.actions.len()
            )));
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
    fn moving_entity(
        &mut self,
        entity: bloxgloom_host_api::motion::MovingEntity,
    ) -> Result<(), RegistrationError> {
        self.room()?;
        entity.validate()?;
        if self.moving.iter().any(|old| old.key == entity.key) {
            return Err(RegistrationError(
                "duplicate moving entity declaration".into(),
            ));
        }
        self.moving.push(entity);
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
            + self.gameplay_entities.len()
            + self.gameplay_handlers.len()
            + self.gameplay_observers.len()
            + self.player_lifecycles.len()
            + self.cubes.len()
            + self.definitions.len()
            + self.screens.len()
            + self.mobiles.len()
            + self.moving.len()
            + self.machines.len()
            + self.anchored.len()
            + self.systems.len()
            + self.actions.len()
            + self.icons.len()
            + self.generation.len()
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
        registration.generation.sort_by(|a, b| a.key.cmp(&b.key));
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
            .gameplay_entities
            .sort_by(|a, b| a.key.cmp(&b.key));
        registration
            .definitions
            .sort_by(|a, b| a.entity.cmp(&b.entity));
        registration
            .machines
            .sort_by(|a, b| a.entity.cmp(&b.entity));
        for mobile in &registration.mobiles {
            candidate.register_mobile(mobile.clone())?;
        }
        registration.moving.sort_by(|a, b| a.key.cmp(&b.key));
        for moving in &registration.moving {
            candidate.register_moving(moving.clone())?;
        }
        for entity in &registration.gameplay_entities {
            candidate.register_gameplay_entity(entity.clone())?;
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
        registration
            .gameplay_handlers
            .sort_by(|a, b| a.key.cmp(&b.key));
        for handler in &registration.gameplay_handlers {
            candidate.register_gameplay_handler(handler.clone())?;
        }
        registration
            .gameplay_observers
            .sort_by(|a, b| a.key.cmp(&b.key));
        registration
            .player_lifecycles
            .sort_by(|a, b| a.key.cmp(&b.key));
        for lifecycle in &registration.player_lifecycles {
            candidate.register_player_lifecycle(lifecycle.clone())?;
        }
        for observer in &registration.gameplay_observers {
            candidate.register_gameplay_observer(observer.clone())?;
        }
        for (_, moving) in candidate.moving_entities() {
            for (enabled, kind) in [
                (
                    moving.handles_impact,
                    bloxgloom_host_api::gameplay::EventKind::MovingImpact,
                ),
                (
                    moving.handles_expiry,
                    bloxgloom_host_api::gameplay::EventKind::MovingExpiry,
                ),
            ] {
                if enabled && candidate.gameplay_handler(kind, &moving.key).is_none() {
                    return Err(RegistrationError(format!(
                        "{}: moving reaction handler missing",
                        moving.key
                    )));
                }
            }
        }
        for action in candidate.registered_actions() {
            if action.operation == bloxgloom_host_api::actions::Operation::Gameplay
                && candidate
                    .gameplay_handler(
                        bloxgloom_host_api::gameplay::EventKind::ActionRequested,
                        &action.key,
                    )
                    .is_none()
            {
                return Err(RegistrationError(format!(
                    "{}: missing gameplay action owner",
                    action.key
                )));
            }
        }
        for entity in candidate.gameplay_entities() {
            if entity.initial_delay_ticks.is_some()
                && candidate
                    .entity_type_id_by_key(&entity.key)
                    .and_then(|id| candidate.moving_entity(id))
                    .is_none()
                && candidate
                    .gameplay_handler(
                        bloxgloom_host_api::gameplay::EventKind::EntityTick,
                        &entity.key,
                    )
                    .is_none()
            {
                return Err(RegistrationError(format!(
                    "{}: scheduled entity requires a tick handler",
                    entity.key
                )));
            }
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
