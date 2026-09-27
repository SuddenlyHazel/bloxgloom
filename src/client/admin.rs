//! Local admin menu commands. Parsing is client convenience; the server still
//! authenticates every grant and commits its inventory change through the WAL.

use super::ClientApp;
use crate::content::Catalog;
use crate::inventory::STACK_LIMIT;
use crate::items::ItemId;
use crate::protocol::ClientMessage;

enum Command {
    Help,
    Give(ItemId, u16),
    Spawn(crate::content::EntityTypeId),
}

fn parse(input: &str, catalog: &Catalog) -> Result<Command, &'static str> {
    let mut parts = input.trim().trim_start_matches('/').split_whitespace();
    match parts.next() {
        Some("help") if parts.next().is_none() => Ok(Command::Help),
        Some("spawn") => {
            let key = parts.next().ok_or("Usage: spawn <entity-key>")?;
            if parts.next().is_some() {
                return Err("Usage: spawn <entity-key>");
            }
            let key = if key.contains(':') {
                key.to_owned()
            } else {
                format!("bloxgloom:{key}")
            };
            catalog
                .entity_type_id_by_key(&key)
                .filter(|id| catalog.mobile_entity(*id).is_some())
                .map(Command::Spawn)
                .ok_or("Unknown creature")
        }
        Some("give") => {
            let key = parts.next().ok_or("Usage: give <item-key> [1..128]")?;
            let count = parts
                .next()
                .map(str::parse::<u16>)
                .transpose()
                .map_err(|_| "Count must be 1..128")?
                .unwrap_or(STACK_LIMIT);
            if parts.next().is_some() || !(1..=STACK_LIMIT).contains(&count) {
                return Err("Usage: give <item-key> [1..128]");
            }
            let mut matches = catalog
                .items()
                .filter(|item| item.key == key || item.key.rsplit(':').next() == Some(key));
            let item = matches.next().ok_or("Unknown item key")?.id;
            if matches.next().is_some() {
                return Err("Ambiguous item key; use namespace:key");
            }
            Ok(Command::Give(item, count))
        }
        _ => Err("Commands: give <item-key> [count], spawn mossbun, help"),
    }
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
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        self.queue_command(ClientMessage::AdminGive {
            action_id,
            item,
            count,
        });
        self.show_status("Grant submitted");
    }

    pub(super) fn admin_run(&mut self) {
        match parse(&self.admin_input, &self.catalog) {
            Ok(Command::Help) => {
                self.show_status("give namespace:item [1..128] / spawn namespace:entity")
            }
            Ok(Command::Give(item, count)) => {
                self.admin_grant(item, count);
                self.admin_input.clear();
            }
            Ok(Command::Spawn(entity_type)) => {
                let Some(action_id) = self.allocate_action_id() else {
                    self.show_status("Action session pending or busy");
                    return;
                };
                self.queue_command(ClientMessage::AdminSpawnEntity {
                    action_id,
                    entity_type,
                });
                self.admin_input.clear();
                self.show_status("Creature spawn submitted; needs nearby clear ground");
            }
            Err(message) => self.show_status(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn give_parser_accepts_namespaced_item_and_rejects_invalid_counts() {
        let catalog = Catalog::builtins();
        let key = catalog.items().next().unwrap().key.to_string();
        let Command::Give(item, count) = parse(&format!("/give {key} 12"), &catalog).unwrap()
        else {
            panic!("expected give")
        };
        assert_eq!(catalog.item(item).unwrap().key, key);
        assert_eq!(count, 12);
        assert!(parse(&format!("give {key} 129"), &catalog).is_err());
        assert!(parse("give madeup:block", &catalog).is_err());
    }

    #[test]
    fn mossbun_spawn_is_one_explicit_creature_not_an_inventory_item() {
        let catalog = Catalog::builtins();
        assert!(matches!(
            parse("/spawn mossbun", &catalog),
            Ok(Command::Spawn(crate::content::MOSSBUN_ENTITY_TYPE))
        ));
        assert!(matches!(
            parse("spawn bloxgloom:mossbun", &catalog),
            Ok(Command::Spawn(crate::content::MOSSBUN_ENTITY_TYPE))
        ));
        assert!(parse("spawn mossbun 100", &catalog).is_err());
        assert!(parse("spawn madeup", &catalog).is_err());
        assert!(parse("give mossbun", &catalog).is_err());
    }
}
