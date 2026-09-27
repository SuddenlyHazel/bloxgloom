use super::*;
use bloxgloom_host_api::{
    InventoryScreen, RegistrationError as ApiError, SlotGroup, StatusField, StatusFormat,
};
use std::sync::Arc;

impl Catalog {
    pub(crate) fn register_inventory_screen(
        &mut self,
        screen: InventoryScreen,
    ) -> Result<(), ApiError> {
        screen.validate()?;
        if self
            .inventory_screens
            .iter()
            .flatten()
            .any(|s| s.block == screen.block)
        {
            return Err(ApiError("duplicate inventory block binding".into()));
        }
        let entity = self
            .entity_type_id_by_key(&screen.entity)
            .ok_or_else(|| ApiError(format!("missing inventory entity {}", screen.entity)))?;
        if self.block_by_key(&screen.block).is_none() {
            return Err(ApiError(format!(
                "missing inventory block {}",
                screen.block
            )));
        }
        self.inventory_screens
            .resize_with(self.entities.len(), || None);
        let entry = &mut self.inventory_screens[entity.0 as usize];
        if entry.is_some() {
            return Err(ApiError("duplicate inventory screen".into()));
        }
        let action = bloxgloom_host_api::actions::Action {
            key: format!("{}/inventory", screen.entity),
            version: 1,
            label: screen.title.clone(),
            target: bloxgloom_host_api::actions::Target::Block(screen.block.clone()),
            operation: bloxgloom_host_api::actions::Operation::Inventory,
            panel: None,
        };
        *entry = Some(Arc::new(screen));
        self.register_action(action)?;
        Ok(())
    }
    pub(crate) fn inventory_screen(&self, entity: EntityTypeId) -> Option<&Arc<InventoryScreen>> {
        self.inventory_screens.get(entity.0 as usize)?.as_ref()
    }
    pub(crate) fn inventory_screens(
        &self,
    ) -> impl Iterator<Item = (EntityTypeId, &Arc<InventoryScreen>)> {
        self.inventory_screens
            .iter()
            .enumerate()
            .filter_map(|(id, screen)| screen.as_ref().map(|s| (EntityTypeId(id as u32), s)))
    }
    #[cfg(test)]
    pub(crate) fn inventory_for_state(&self, state: BlockStateId) -> Option<&Arc<InventoryScreen>> {
        let block = &self.block_type(self.state(state)?.block_type)?.key;
        self.inventory_screens
            .iter()
            .flatten()
            .find(|screen| screen.block == *block)
    }
    pub(super) fn builtin_inventory_screens(&mut self) {
        let mut kiln = InventoryScreen::storage(
            "bloxgloom:kiln",
            "bloxgloom:kiln",
            "KILN",
            3,
            3,
            vec![[0, 0, 0], [0, 1, 0]],
        );
        kiln.hint = "GRAVEL > STONE / FUEL: WOOD, STICKS, SAPLINGS".into();
        kiln.groups = [("FUEL", true), ("INPUT", true), ("OUTPUT", false)]
            .into_iter()
            .enumerate()
            .map(|(i, (label, insert))| SlotGroup {
                label: label.into(),
                first: i as u8,
                count: 1,
                insert,
                extract: true,
            })
            .collect();
        kiln.status = vec![
            StatusField {
                label: "FUEL".into(),
                format: StatusFormat::Milliseconds,
                maximum: 96_000,
            },
            StatusField {
                label: "COOKING".into(),
                format: StatusFormat::Progress,
                maximum: 1000,
            },
        ];
        self.register_inventory_screen(kiln)
            .expect("builtin kiln screen");
        let mut hopper = InventoryScreen::storage(
            "bloxgloom:hopper",
            "bloxgloom:hopper",
            "HOPPER",
            3,
            3,
            vec![[0; 3]],
        );
        hopper.hint = "PULLS FROM ABOVE / FEEDS BELOW / ONE PER PULSE".into();
        self.register_inventory_screen(hopper)
            .expect("builtin hopper screen");
        self.register_inventory_screen(InventoryScreen::storage(
            "bloxgloom:chest",
            "bloxgloom:chest",
            "CHEST",
            27,
            9,
            vec![[0; 3]],
        ))
        .expect("builtin chest screen");
    }
}
