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
}

fn parse(input: &str, catalog: &Catalog) -> Result<Command, &'static str> {
    let mut parts = input.trim().trim_start_matches('/').split_whitespace();
    match parts.next() {
        Some("help") if parts.next().is_none() => Ok(Command::Help),
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
        _ => Err("Commands: give <item-key> [count], help"),
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
                self.show_status("give namespace:item [1..128]  /  click an item to grant 128")
            }
            Ok(Command::Give(item, count)) => {
                self.admin_grant(item, count);
                self.admin_input.clear();
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
}
