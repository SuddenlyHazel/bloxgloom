//! Generic registered action discovery, composition and durable request controls.
use super::*;
use bloxgloom_host_api::actions::{
    Action, Operation, Panel, Request, Target, TerrainRequest, Widget,
};

#[derive(Clone)]
pub(super) struct ActionChoice {
    pub action: Arc<Action>,
    pub request: Request,
    pub target: [i32; 3],
}

impl ClientApp {
    pub(super) fn pump_package_action(&mut self) {
        let Some((key, arguments)) = self
            .package_ui
            .as_mut()
            .and_then(|ui| ui.take_action_request())
        else {
            return;
        };
        // Validate selection before allocating: a locally rejected request must
        // not leave a hole in the server-issued receipt sequence.
        let Some(mut request) = self.compose_current_package_action_with_args(&key, arguments)
        else {
            self.package_ui
                .as_mut()
                .unwrap()
                .action_failed_locally("select a matching item or aim at a matching target");
            return;
        };
        let Some(action_id) = self.allocate_action_id() else {
            self.package_ui
                .as_mut()
                .unwrap()
                .action_failed_locally("action session pending or busy");
            return;
        };
        let ClientMessage::EntityInteract { action_id: id, .. } = &mut request else {
            unreachable!()
        };
        *id = action_id;
        self.queue_command(request);
        self.package_ui
            .as_mut()
            .unwrap()
            .action_submitted(action_id);
    }

    #[cfg(test)]
    fn compose_current_package_action(&self, key: &str) -> Option<ClientMessage> {
        self.compose_current_package_action_with_args(key, vec![])
    }

    fn compose_current_package_action_with_args(
        &self,
        key: &str,
        arguments: Vec<u8>,
    ) -> Option<ClientMessage> {
        let action = self.catalog.action(key)?;
        if let Target::Entity(_) = &action.target {
            let camera = self.camera();
            let limit = self.aimed_block().map_or(7.0, |hit| hit.distance);
            let entity = self.replicas.aimed_mobile(
                &self.catalog,
                camera.position,
                camera.direction(),
                limit,
            )?;
            return compose_observed_entity_action(
                &self.catalog,
                action,
                entity,
                self.config.selected_slot as u8,
                self.inventory.revision,
                arguments,
                1u128 << 64 | 1,
            );
        }
        compose_package_action_with_args(
            &self.catalog,
            key,
            &self.inventory,
            PackageActionInput {
                slot: self.config.selected_slot as u8,
                target: self.position.to_array().map(|v| v.floor() as i32),
                aimed: self.aimed_block().and_then(|hit| {
                    let [x, y, z] = hit.block;
                    let chunk = self.chunks.get(&crate::world::world_to_chunk(x, y, z).0)?;
                    Some((hit, chunk.version))
                }),
                action_id: 1u128 << 64 | 1,
                arguments,
            },
        )
    }
    pub(super) fn action_panel(&self) -> Option<Panel> {
        if self.screen != UiScreen::Actions {
            return None;
        }
        if let Some(index) = self.active_action {
            return self.action_choices.get(index)?.action.panel.clone();
        }
        Some(Panel {
            title: "ACTIONS".into(),
            widgets: self
                .action_choices
                .iter()
                .map(|choice| Widget::Button {
                    action: None,
                    label: choice.action.label.clone(),
                    tooltip: "Select this registered action".into(),
                })
                .collect(),
        })
    }
    pub(super) fn open_item_actions(&mut self) -> bool {
        let target = self.inventory.slots[self.config.selected_slot]
            .as_ref()
            .and_then(|stack| self.catalog.item(stack.item))
            .map(|item| Target::Item(item.key.to_string()))
            .unwrap_or(Target::Empty);
        // Console commands carry their own validated arguments and menu; do not
        // offer the argument-less descriptors as generic action buttons.
        let visible = |action: &&Arc<Action>| {
            action.key != crate::gameplay::admin::GIVE
                && action.key != crate::gameplay::admin::SPAWN
                && action.key != crate::gameplay::admin::TIME
                && action.command.is_none()
                && action.key != crate::gameplay::drop_stack::KEY
                && action.key != crate::gameplay::slot_move::KEY
        };
        let actions: Vec<_> = self
            .catalog
            .discover_actions(&target)
            .filter(visible)
            .cloned()
            .collect();
        let actions = if actions.is_empty() {
            self.catalog
                .discover_actions(&Target::Empty)
                .filter(visible)
                .cloned()
                .collect()
        } else {
            actions
        };
        if actions.is_empty() {
            return false;
        }
        self.action_choices = actions
            .into_iter()
            .map(|action| ActionChoice {
                request: Request {
                    key: action.key.clone(),
                    version: action.version,
                    slot: self.config.selected_slot as u8,
                    inventory_revision: self.inventory.revision,
                    entity: 0,
                    entity_revision: 0,
                    arguments: vec![],
                },
                action,
                target: self.position.to_array().map(|v| v.floor() as i32),
            })
            .collect();
        self.active_action = (self.action_choices.len() == 1
            && self.action_choices[0].action.panel.is_some())
        .then_some(0);
        self.set_screen(UiScreen::Actions);
        true
    }
    pub(super) fn action_control(&mut self, row: u8) {
        if self.screen != UiScreen::Actions {
            return;
        }
        let Some(panel) = self.action_panel() else {
            return;
        };
        if !matches!(
            panel.widgets.get(usize::from(row)),
            Some(Widget::Button { .. })
        ) {
            return;
        }
        let index = self.active_action.unwrap_or(usize::from(row));
        let Some(mut choice) = self.action_choices.get(index).cloned() else {
            return;
        };
        if self.active_action.is_some()
            && let Widget::Button {
                action: Some(key), ..
            } = &panel.widgets[usize::from(row)]
        {
            let Some(action) = self
                .catalog
                .action(key)
                .cloned()
                .filter(|a| a.target == choice.action.target)
            else {
                return;
            };
            choice.request.key = action.key.clone();
            choice.request.version = action.version;
            choice.action = action;
        }
        if choice.action.operation == bloxgloom_host_api::actions::Operation::Inventory {
            if let Some(state) = self.block_at(choice.target[0], choice.target[1], choice.target[2])
            {
                self.open_inventory_at(choice.target, state);
            }
            return;
        }
        if self.active_action.is_none() && choice.action.panel.is_some() {
            self.active_action = Some(index);
            self.refresh_layout();
            return;
        }
        self.send_registered(choice);
        self.set_screen(UiScreen::Playing);
    }
    pub(super) fn send_registered(&mut self, choice: ActionChoice) {
        let terrain_version = if choice.action.operation == Operation::Gameplay
            && let Target::Block(key) = &choice.action.target
        {
            let [x, y, z] = choice.target;
            let Some((state, version)) = self
                .block_at(x, y, z)
                .zip(self.chunks.get(&crate::world::world_to_chunk(x, y, z).0))
                .map(|(state, chunk)| (state, chunk.version))
            else {
                self.show_status("Target terrain is still loading");
                return;
            };
            if self
                .catalog
                .state(state)
                .and_then(|state| self.catalog.block_type(state.block_type))
                .is_none_or(|block| block.key != *key)
            {
                self.show_status("Target block changed");
                return;
            }
            Some(version)
        } else {
            None
        };
        let payload = match terrain_version {
            Some(version) => TerrainRequest {
                version,
                request: choice.request,
            }
            .encode(),
            None => choice.request.encode(),
        };
        let Some(payload) = payload else {
            self.show_status("Invalid action request");
            return;
        };
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target: choice.target,
            payload,
        });
    }
}

fn compose_observed_entity_action(
    catalog: &crate::content::Catalog,
    action: &Action,
    entity: &crate::protocol::PublicEntity,
    slot: u8,
    inventory_revision: u64,
    arguments: Vec<u8>,
    action_id: u128,
) -> Option<ClientMessage> {
    let Target::Entity(expected) = &action.target else {
        return None;
    };
    if action.operation != Operation::Gameplay
        || &catalog.entity_type(entity.entity_type)?.key != expected
    {
        return None;
    }
    let crate::protocol::PublicEntityLocation::Mobile { position } = entity.location else {
        return None;
    };
    let request = Request {
        key: action.key.clone(),
        version: action.version,
        slot,
        inventory_revision,
        entity: entity.id,
        entity_revision: entity.revision,
        arguments,
    };
    Some(ClientMessage::EntityInteract {
        action_id,
        target: position.map(|v| v.floor() as i32),
        payload: request.encode()?,
    })
}

// The script supplies its owned key and bounded argument bytes. The caller supplies
// current client selection and observed target identity, never script coordinates.
// Server authorization still owns reach, sight, costs and target validation.
pub(crate) fn compose_package_action(
    catalog: &crate::content::Catalog,
    key: &str,
    slot: u8,
    inventory: &crate::inventory::Inventory,
    target: [i32; 3],
    aimed: Option<(Hit, u64)>,
    action_id: u128,
) -> Option<ClientMessage> {
    compose_package_action_with_args(
        catalog,
        key,
        inventory,
        PackageActionInput {
            slot,
            target,
            aimed,
            action_id,
            arguments: vec![],
        },
    )
}

struct PackageActionInput {
    slot: u8,
    target: [i32; 3],
    aimed: Option<(Hit, u64)>,
    action_id: u128,
    arguments: Vec<u8>,
}

fn compose_package_action_with_args(
    catalog: &crate::content::Catalog,
    key: &str,
    inventory: &crate::inventory::Inventory,
    input: PackageActionInput,
) -> Option<ClientMessage> {
    let PackageActionInput {
        slot,
        mut target,
        aimed,
        action_id,
        arguments,
    } = input;
    let action = catalog.action(key)?;
    if action.operation != Operation::Gameplay {
        return None;
    }
    let mut terrain_version = None;
    match &action.target {
        Target::Empty => {}
        Target::Item(item) => {
            let stack = inventory.slots.get(usize::from(slot))?.as_ref()?;
            if catalog.item(stack.item)?.key != *item {
                return None;
            }
        }
        Target::Block(key) => {
            let (hit, version) = aimed?;
            let state = catalog.state(hit.block_id)?;
            if catalog.block_type(state.block_type)?.key != *key {
                return None;
            }
            target = hit.block;
            terrain_version = Some(version);
        }
        Target::Entity(_) => return None,
    }
    let request = Request {
        key: action.key.clone(),
        version: action.version,
        slot,
        inventory_revision: inventory.revision,
        entity: 0,
        entity_revision: 0,
        arguments,
    };
    Some(ClientMessage::EntityInteract {
        action_id,
        target,
        payload: match terrain_version {
            Some(version) => TerrainRequest { version, request }.encode()?,
            None => request.encode()?,
        },
    })
}

/// Resolve a persisted local shortcut against this session's negotiated
/// command contract before allocating a receipt sequence.
pub(crate) fn compose_named_command(
    catalog: &crate::content::Catalog,
    key: &str,
    slot: u8,
    inventory: &crate::inventory::Inventory,
    position: [i32; 3],
) -> Option<ClientMessage> {
    let action = catalog.action(key)?;
    if !action
        .command
        .as_ref()
        .is_some_and(|command| command.arguments.is_empty())
        || action.target != Target::Empty
    {
        return None;
    }
    compose_package_action(
        catalog,
        key,
        slot,
        inventory,
        position,
        None,
        1u128 << 64 | 1,
    )
}

#[cfg(test)]
pub(crate) mod tests;
