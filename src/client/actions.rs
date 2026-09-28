//! Generic registered action discovery, composition and durable request controls.
use super::*;
use bloxgloom_host_api::actions::{Action, Operation, Panel, Request, Target, Widget};

#[derive(Clone)]
pub(super) struct ActionChoice {
    pub action: Arc<Action>,
    pub request: Request,
    pub target: [i32; 3],
}

impl ClientApp {
    pub(super) fn pump_package_action(&mut self) {
        let Some(key) = self.package_ui.as_mut().and_then(|ui| ui.take_action()) else {
            return;
        };
        let slot = self.config.selected_slot as u8;
        // Validate selection before allocating: a locally rejected request must
        // not leave a hole in the server-issued receipt sequence.
        let Some(mut request) = compose_package_action(
            &self.catalog,
            &key,
            slot,
            &self.inventory,
            self.position.to_array().map(|v| v.floor() as i32),
            1u128 << 64 | 1,
        ) else {
            self.package_ui
                .as_mut()
                .unwrap()
                .action_failed_locally("select a matching item");
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
        let actions: Vec<_> = self.catalog.discover_actions(&target).cloned().collect();
        let actions = if actions.is_empty() {
            self.catalog
                .discover_actions(&Target::Empty)
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
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target: choice.target,
            payload: choice.request.encode().expect("validated action request"),
        });
    }
}

// Only compose a registered item/empty action. The script supplies its own key,
// never an inventory revision, effect, entity identity, or target authority.
pub(crate) fn compose_package_action(
    catalog: &crate::content::Catalog,
    key: &str,
    slot: u8,
    inventory: &crate::inventory::Inventory,
    target: [i32; 3],
    action_id: u128,
) -> Option<ClientMessage> {
    let action = catalog.action(key)?;
    if action.operation != Operation::Gameplay {
        return None;
    }
    match &action.target {
        Target::Empty => {}
        Target::Item(item) => {
            let stack = inventory.slots.get(usize::from(slot))?.as_ref()?;
            if catalog.item(stack.item)?.key != *item {
                return None;
            }
        }
        _ => return None,
    }
    let request = Request {
        key: action.key.clone(),
        version: action.version,
        slot,
        inventory_revision: inventory.revision,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    Some(ClientMessage::EntityInteract {
        action_id,
        target,
        payload: request.encode()?,
    })
}
