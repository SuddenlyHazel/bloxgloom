//! Local aliases/help plus negotiated, schema-driven command requests. Parsing
//! is convenience only: frozen server descriptors authorize durable dispatch.
use super::ClientApp;
use crate::config::bindings::{self, Action as Builtin};
use crate::content::Catalog;
use crate::inventory::STACK_LIMIT;
use crate::items::ItemId;
use crate::protocol::ClientMessage;
use bloxgloom_host_api::actions::{MAX_COMMAND_ARGUMENTS, Request};
use winit::keyboard::KeyCode;

pub(super) const BINDING_ROWS_PER_PAGE: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BindingTarget {
    Builtin(Builtin),
    Named(String),
    Absent(String),
}

impl BindingTarget {
    fn label(&self) -> &str {
        match self {
            Self::Builtin(Builtin::Inventory) => "Inventory",
            Self::Builtin(Builtin::KilnInput) => "Kiln input",
            Self::Builtin(Builtin::KilnFuel) => "Kiln fuel",
            Self::Builtin(Builtin::Drop) => "Drop",
            Self::Named(key) | Self::Absent(key) => key,
        }
    }

    fn capture(&self, key: KeyCode, config: &mut crate::config::Config) -> bool {
        match self {
            Self::Builtin(action) => config.bindings.bind(*action, key, &config.named_bindings),
            Self::Named(action) => config.named_bindings.bind(action, key, config.bindings),
            Self::Absent(_) => false,
        }
    }
}

fn binding_targets(catalog: &Catalog, named: &bindings::NamedBindings) -> Vec<BindingTarget> {
    let mut targets = vec![
        BindingTarget::Builtin(Builtin::Inventory),
        BindingTarget::Builtin(Builtin::KilnInput),
        BindingTarget::Builtin(Builtin::KilnFuel),
        BindingTarget::Builtin(Builtin::Drop),
    ];
    let mut discovered: Vec<_> = catalog
        .registered_actions()
        .filter(|action| {
            action.target == bloxgloom_host_api::actions::Target::Empty
                && action
                    .command
                    .as_ref()
                    .is_some_and(|command| command.arguments.is_empty())
        })
        .map(|action| action.key.clone())
        .collect();
    discovered.sort();
    targets.extend(discovered.iter().cloned().map(BindingTarget::Named));
    targets.extend(
        named
            .0
            .keys()
            .filter(|key| discovered.binary_search(key).is_err())
            .cloned()
            .map(BindingTarget::Absent),
    );
    targets
}

mod players;
#[cfg(test)]
mod tests;
mod time;

enum Command {
    Help,
    Appearance([u8; 3]),
    Time(u64),
    Registered(Request),
}

fn registered(catalog: &Catalog, key: &str, values: &[&str]) -> Result<Request, &'static str> {
    let action = catalog.action(key).ok_or("Unknown command")?;
    let command = action.command.as_ref().ok_or("Action is not a command")?;
    let arguments = command
        .encode_arguments(values)
        .ok_or("Invalid command arguments")?;
    catalog
        .command_arguments(command, &arguments)
        .ok_or("Unknown item or entity key")?;
    Ok(Request {
        key: action.key.clone(),
        version: action.version,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments,
    })
}

#[cfg(test)]
fn parse(input: &str, catalog: &Catalog) -> Result<Command, &'static str> {
    parse_with_players(input, catalog, &[])
}
fn parse_with_players(
    input: &str,
    catalog: &Catalog,
    roster: &[crate::protocol::PlayerSummary],
) -> Result<Command, &'static str> {
    // Bound token traversal/capture even for callers outside the text widget.
    if input.len() > 1024 {
        return Err("Command is too long");
    }
    let mut parts = input.trim().trim_start_matches('/').split_whitespace();
    let key = parts.next().ok_or("Enter a command or help")?;
    let mut values = parts.take(MAX_COMMAND_ARGUMENTS + 1).collect::<Vec<_>>();
    if values.len() > MAX_COMMAND_ARGUMENTS {
        return Err("Too many command arguments");
    }
    let normalized;
    let key = match key {
        "help" if values.is_empty() => return Ok(Command::Help),
        "time" => return time::parse(&values).map(Command::Time),
        "appearance" => {
            if values.len() != 3 {
                return Err("Usage: appearance <skin> <shirt> <pants>");
            }
            let mut palettes = [0; 3];
            for (palette, value) in palettes.iter_mut().zip(&values) {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err("Appearance requires registered palette indices, not colors");
                }
                *palette = value
                    .parse()
                    .map_err(|_| "Invalid appearance palette index")?;
            }
            if !catalog.valid_appearance([palettes[0], palettes[1], palettes[2], 0]) {
                return Err("Unregistered appearance palette; use help for index ranges");
            }
            return Ok(Command::Appearance(palettes));
        }
        "give" => {
            let name = *values.first().ok_or("Usage: give <item-key> [1..128]")?;
            let mut items = catalog
                .items()
                .filter(|item| item.key == name || item.key.rsplit(':').next() == Some(name));
            normalized = items.next().ok_or("Unknown item key")?.key.to_string();
            if items.next().is_some() {
                return Err("Ambiguous item key; use namespace:key");
            }
            values[0] = &normalized;
            crate::gameplay::admin::GIVE
        }
        "spawn" => {
            let name = *values.first().ok_or("Usage: spawn <entity-key>")?;
            normalized = if name.contains(':') {
                name.into()
            } else {
                format!("bloxgloom:{name}")
            };
            let entity = catalog
                .entity_type_id_by_key(&normalized)
                .ok_or("Unknown creature")?;
            if catalog.mobile_entity(entity).is_none() {
                return Err("Unknown creature");
            }
            values[0] = &normalized;
            crate::gameplay::admin::SPAWN
        }
        key => key,
    };
    let values = players::normalize(catalog, key, &values, roster)?;
    let values = values.iter().map(String::as_str).collect::<Vec<_>>();
    registered(catalog, key, &values).map(Command::Registered)
}

impl ClientApp {
    pub(super) fn binding_targets(&self) -> Vec<BindingTarget> {
        binding_targets(&self.catalog, &self.config.named_bindings)
    }

    pub(super) fn binding_view(&self) -> Option<String> {
        if !self.admin_binding_mode || self.screen != crate::ui::UiScreen::Admin {
            return None;
        }
        let targets = self.binding_targets();
        let mut display = crate::ui::UiFrame::BINDING_VIEW_PREFIX.to_owned();
        for (index, target) in targets
            .iter()
            .enumerate()
            .skip(self.admin_binding_page * BINDING_ROWS_PER_PAGE)
            .take(BINDING_ROWS_PER_PAGE)
        {
            let key = match target {
                BindingTarget::Builtin(Builtin::Inventory) => Some(self.config.bindings.inventory),
                BindingTarget::Builtin(Builtin::KilnInput) => Some(self.config.bindings.kiln_input),
                BindingTarget::Builtin(Builtin::KilnFuel) => Some(self.config.bindings.kiln_fuel),
                BindingTarget::Builtin(Builtin::Drop) => Some(self.config.bindings.drop),
                BindingTarget::Named(key) | BindingTarget::Absent(key) => {
                    self.config.named_bindings.0.get(key).copied()
                }
            };
            let label: String = target.label().chars().take(65).collect();
            display.push('\n');
            display.push_str(&format!(
                "{label}  :  {}{}{}",
                key.and_then(bindings::letter)
                    .map_or("-".to_owned(), |letter| letter.to_string()),
                if matches!(target, BindingTarget::Absent(_)) {
                    "  (NOT IN SESSION)"
                } else {
                    ""
                },
                if self.admin_binding_selected == Some(index) {
                    "  [PRESS LETTER]"
                } else {
                    ""
                }
            ));
        }
        Some(display)
    }

    pub(super) fn binding_select(&mut self, row: u8) {
        let index = self.admin_binding_page * BINDING_ROWS_PER_PAGE + usize::from(row);
        match self.binding_targets().get(index) {
            Some(BindingTarget::Builtin(_) | BindingTarget::Named(_)) => {
                self.admin_binding_selected = Some(index);
                self.show_status("Press a distinct non-movement letter; Esc cancels");
            }
            Some(BindingTarget::Absent(_)) => {
                self.admin_binding_selected = None;
                self.show_status("Not in this session");
            }
            None => {}
        }
    }

    pub(super) fn binding_capture(&mut self, key: KeyCode) {
        let Some(index) = self.admin_binding_selected else {
            return;
        };
        // Re-resolve at capture: a stale row cannot authorize a command that
        // this session never negotiated.
        let valid = self
            .binding_targets()
            .get(index)
            .is_some_and(|target| target.capture(key, &mut self.config));
        if valid {
            self.admin_binding_selected = None;
            self.config_writer.request_save(&self.config);
            self.show_status("Binding saved");
        } else {
            self.show_status("Invalid or already bound key");
        }
    }

    pub(super) fn admin_grant_index(&mut self, index: u8) {
        let item = self
            .catalog
            .items()
            .nth(self.admin_page * 24 + usize::from(index))
            .map(|item| item.id);
        if let Some(item) = item {
            self.admin_grant(item, STACK_LIMIT);
        }
    }

    fn admin_grant(&mut self, item: ItemId, count: u16) {
        let Some(item) = self.catalog.item(item) else {
            return;
        };
        match registered(
            &self.catalog,
            crate::gameplay::admin::GIVE,
            &[&item.key, &count.to_string()],
        ) {
            Ok(request) => self.submit_admin_command(request),
            Err(message) => self.show_status(message),
        }
    }

    fn submit_admin_command(&mut self, mut request: Request) {
        request.inventory_revision = self.inventory.revision;
        let Some(payload) = request.encode() else {
            self.show_status("Invalid command request");
            return;
        };
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target: [0; 3],
            payload,
        });
        self.admin_input.clear();
        self.show_status("Command submitted");
    }

    pub(super) fn complete_player_command(&mut self) {
        match players::complete(&self.admin_input, &self.catalog, &self.player_roster) {
            Ok(input) => self.admin_input = input,
            Err(message) => self.show_status(message),
        }
    }
    pub(super) fn admin_run(&mut self) {
        match parse_with_players(&self.admin_input, &self.catalog, &self.player_roster) {
            Ok(Command::Help) => {
                let commands = self
                    .catalog
                    .registered_actions()
                    .filter(|action| action.command.is_some())
                    .map(|action| action.key.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                let maxima = std::array::from_fn::<_, 3, _>(|part| {
                    (0..32)
                        .take_while(|index| self.catalog.appearance_color(part, *index).is_some())
                        .count()
                        - 1
                });
                self.show_status(format!(
                    "time set <sunrise|noon|sunset|midnight|HH:MM> / appearance <skin 0..{}> <shirt 0..{}> <pants 0..{}> / {commands}",
                    maxima[0], maxima[1], maxima[2]
                ));
            }
            Ok(Command::Appearance(palettes)) => {
                // Profile cosmetics are a local command, not a registered
                // gameplay action and never consume a world-WAL action ID.
                self.queue_command(ClientMessage::SelectAppearance { palettes });
                self.admin_input.clear();
                self.show_status("Appearance selection submitted");
            }
            Ok(Command::Time(elapsed_ms)) => {
                self.submit_admin_command(Request {
                    key: crate::gameplay::admin::TIME.into(),
                    version: 1,
                    slot: 0,
                    inventory_revision: self.inventory.revision,
                    entity: 0,
                    entity_revision: 0,
                    arguments: elapsed_ms.to_le_bytes().to_vec(),
                });
            }
            Ok(Command::Registered(request)) => self.submit_admin_command(request),
            Err(message) => self.show_status(message),
        }
    }
}
