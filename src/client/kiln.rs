//! Workstation controls over replicated entity state and durable interactions.
use super::*;
use crate::protocol::workstation::WorkstationView;

impl ClientApp {
    pub(super) fn open_aimed_kiln(&mut self) -> bool {
        let Some(hit) = self.aimed_block() else {
            return false;
        };
        let block = &self
            .catalog
            .block_type(self.catalog.state(hit.block_id).unwrap().block_type)
            .unwrap()
            .key;
        let mut choices: Vec<_> = self
            .catalog
            .discover_actions(&bloxgloom_host_api::actions::Target::Block(
                block.to_string(),
            ))
            .cloned()
            .collect();
        if choices.is_empty()
            && let Some(entity) = self.replicas.action_at(hit.block, &self.catalog)
        {
            let key = &self.catalog.entity_type(entity.entity_type).unwrap().key;
            choices = self
                .catalog
                .discover_actions(&bloxgloom_host_api::actions::Target::Entity(
                    key.to_string(),
                ))
                .cloned()
                .collect();
        }
        if choices.is_empty() {
            return false;
        }
        if choices.len() == 1
            && choices[0].operation == bloxgloom_host_api::actions::Operation::Inventory
        {
            return self.open_inventory_at(hit.block, hit.block_id);
        }
        let entity = self.replicas.action_at(hit.block, &self.catalog);
        let uses_entity = |action: &bloxgloom_host_api::actions::Action| {
            !(matches!(action.target, bloxgloom_host_api::actions::Target::Block(_))
                && action.operation == bloxgloom_host_api::actions::Operation::Gameplay)
        };
        if entity.is_none() && choices.iter().any(|action| uses_entity(action)) {
            self.show_status("Target state is still loading");
            return true;
        }
        self.action_choices = choices
            .into_iter()
            .map(|action| {
                let identity = entity.filter(|_| uses_entity(&action));
                actions::ActionChoice {
                    request: bloxgloom_host_api::actions::Request {
                        key: action.key.clone(),
                        version: action.version,
                        slot: self.config.selected_slot as u8,
                        inventory_revision: self.inventory.revision,
                        entity: identity.map_or(0, |entity| entity.id),
                        entity_revision: identity.map_or(0, |entity| entity.revision),
                        arguments: vec![],
                    },
                    action,
                    target: hit.block,
                }
            })
            .collect();
        self.active_action = (self.action_choices.len() == 1
            && self.action_choices[0].action.panel.is_some())
        .then_some(0);
        self.set_screen(UiScreen::Actions);
        true
    }

    pub(super) fn open_inventory_at(
        &mut self,
        target: [i32; 3],
        state: crate::content::BlockStateId,
    ) -> bool {
        if self.catalog.inventory_action(state).is_none() {
            return false;
        }
        let Some(entity) = self.replicas.kiln_at(target, &self.catalog) else {
            self.show_status("Workstation state is still loading");
            return true;
        };
        let id = entity.id;
        self.kiln_target = Some((target, id));
        self.set_screen(UiScreen::Container);
        true
    }

    fn current_kiln(&self) -> Option<&crate::protocol::PublicEntity> {
        let (target, id) = self.kiln_target?;
        self.replicas
            .kiln_at(target, &self.catalog)
            .filter(|entity| entity.id == id)
    }

    pub(super) fn kiln_view(&self) -> Option<WorkstationView> {
        let entity = self.current_kiln()?;
        let screen = self.catalog.inventory_screen(entity.entity_type)?;
        WorkstationView::decode(&entity.payload).filter(|v| v.valid_for(screen, &self.catalog))
    }

    pub(super) fn container_screen(
        &self,
    ) -> Option<std::sync::Arc<bloxgloom_host_api::InventoryScreen>> {
        self.catalog
            .inventory_screen(self.current_kiln()?.entity_type)
            .cloned()
    }

    pub(super) fn validate_kiln_screen(&mut self) {
        if self.screen != UiScreen::Container {
            return;
        }
        let valid = self.kiln_target.is_some_and(|(cell, _)| {
            self.camera()
                .position
                .distance(Vec3::from_array(cell.map(|v| v as f32 + 0.5)))
                < 8.0
        }) && self.kiln_view().is_some()
            && !self.disconnected;
        if !valid {
            self.set_screen(UiScreen::Playing);
            self.show_status("Workstation is no longer available");
        }
    }

    pub(super) fn kiln_click(&mut self, slot: u8, one: bool) {
        if self
            .kiln_view()
            .is_none_or(|v| usize::from(slot) >= v.slots.len())
        {
            return;
        }
        if let Some(inventory) = self.inventory_source {
            if self
                .container_screen()
                .is_none_or(|s| s.group(slot).is_none_or(|g| !g.insert))
            {
                self.show_status("Slot does not accept items");
                return;
            }
            self.kiln_transfer(
                0,
                slot,
                inventory,
                if one {
                    1
                } else {
                    crate::inventory::STACK_LIMIT
                },
            );
        } else {
            if self
                .container_screen()
                .is_none_or(|s| s.group(slot).is_none_or(|g| !g.extract))
            {
                self.show_status("Slot is deposit only");
                return;
            }
            self.kiln_source = if self.kiln_source == Some(slot) {
                None
            } else {
                Some(slot)
            };
        }
    }

    pub(super) fn kiln_inventory_click(&mut self, slot: u8, one: bool) {
        if usize::from(slot) >= crate::inventory::SLOTS {
            return;
        }
        if let Some(source) = self.kiln_source {
            self.kiln_transfer(
                1,
                source,
                slot,
                if one {
                    1
                } else {
                    crate::inventory::STACK_LIMIT
                },
            );
        } else {
            self.inventory_source = if self.inventory_source == Some(slot) {
                None
            } else {
                Some(slot)
            };
        }
    }

    fn kiln_transfer(&mut self, operation: u8, kiln_slot: u8, inventory_slot: u8, count: u16) {
        if count == 0 {
            self.show_status("Source empty or destination full");
            return;
        }
        let Some(entity) = self.current_kiln() else {
            return;
        };
        let screen = self.catalog.inventory_screen(entity.entity_type).unwrap();
        let action = self
            .catalog
            .action(&format!("{}/inventory", screen.entity))
            .unwrap();
        let mut arguments = vec![operation, kiln_slot];
        arguments.extend(count.to_le_bytes());
        let payload = bloxgloom_host_api::actions::Request {
            key: action.key.clone(),
            version: action.version,
            slot: inventory_slot,
            inventory_revision: self.inventory.revision,
            entity: entity.id,
            entity_revision: entity.revision,
            arguments,
        }
        .encode()
        .unwrap();
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        let target = self.kiln_target.unwrap().0;
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target,
            payload,
        });
        self.inventory_source = None;
        self.kiln_source = None;
    }
}
