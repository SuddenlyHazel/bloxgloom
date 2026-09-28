//! Local aliases/help plus negotiated, schema-driven command requests. Parsing
//! is convenience only: frozen server descriptors authorize durable dispatch.
use super::ClientApp;
use crate::content::Catalog;
use crate::inventory::STACK_LIMIT;
use crate::items::ItemId;
use crate::protocol::ClientMessage;
use bloxgloom_host_api::actions::{MAX_COMMAND_ARGUMENTS, Request};

#[cfg(test)]
mod tests;

enum Command {
    Help,
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

fn parse(input: &str, catalog: &Catalog) -> Result<Command, &'static str> {
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
    registered(catalog, key, &values).map(Command::Registered)
}

impl ClientApp {
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

    pub(super) fn admin_run(&mut self) {
        match parse(&self.admin_input, &self.catalog) {
            Ok(Command::Help) => self.show_status(
                "give namespace:item [1..128] / spawn namespace:entity / namespace:command",
            ),
            Ok(Command::Registered(request)) => self.submit_admin_command(request),
            Err(message) => self.show_status(message),
        }
    }
}
