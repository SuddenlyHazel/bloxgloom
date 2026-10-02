//! Frozen action index shared by discovery and authoritative dispatch.
use super::*;
use bloxgloom_host_api::{RegistrationError as Error, actions::*};
use std::sync::Arc;
#[cfg(test)]
mod tests;

impl Catalog {
    /// Decode and resolve every typed reference against this frozen catalog.
    /// The durable dispatcher calls this before any gameplay handler.
    pub(crate) fn command_arguments(
        &self,
        command: &Command,
        bytes: &[u8],
    ) -> Option<Vec<CommandValue>> {
        let values = command.decode_arguments(bytes)?;
        values
            .iter()
            .all(|value| match value {
                CommandValue::ItemKey(key) => self.item_by_key(key).is_some(),
                CommandValue::EntityKey(key) => self.entity_type_id_by_key(key).is_some(),
                CommandValue::Count(_)
                | CommandValue::Player { .. }
                | CommandValue::Text(_)
                | CommandValue::Integer(_)
                | CommandValue::Number(_) => true,
            })
            .then_some(values)
    }

    pub(crate) fn register_action(&mut self, action: Action) -> Result<(), Error> {
        action.validate()?;
        if self.entity_type_id_by_key("bloxgloom:player").is_none() {
            return Err(Error(
                "actions require the host player compatibility contract".into(),
            ));
        }
        let exists = match &action.target {
            Target::Empty => true,
            Target::Item(k) => self.items().any(|i| i.key == *k),
            Target::Block(k) => self.block_by_key(k).is_some(),
            Target::Entity(k) => self.entity_type_id_by_key(k).is_some(),
        };
        let operation = match &action.operation {
            Operation::Gameplay => true,
            Operation::Recipe { input, output, .. } => [input, output].iter().all(|k| {
                self.items()
                    .any(|i| &*i.key == k.as_str() && self.valid_item_components(i.id, None))
            }),
            Operation::Inventory => {
                matches!(&action.target,Target::Block(k) if self.inventory_screens().any(|(_,s)| s.block == *k))
            }
            Operation::EntityRequest(_) => match &action.target {
                Target::Entity(k) => self.entity_type_id_by_key(k).is_some(),
                Target::Block(k) => self.block_by_key(k).is_some(),
                _ => false,
            },
        };
        if !exists || !operation {
            return Err(Error("unresolved action references".into()));
        }
        let output = match &action.operation {
            Operation::Recipe { output, .. } => {
                Some(self.items().find(|i| i.key == *output).unwrap().id)
            }
            _ => None,
        };
        let key = action.key.clone();
        self.actions.register(action)?;
        if let Some(output) = output {
            self.action_outputs.insert(key, output);
        }
        Ok(())
    }
    pub(crate) fn action_output(&self, key: &str) -> Option<ItemId> {
        self.action_outputs.get(key).copied()
    }
    pub(crate) fn action(&self, key: &str) -> Option<&Arc<Action>> {
        self.actions.get(key)
    }
    /// Alias lookup is for client convenience only. Authoritative dispatch uses action().
    pub(crate) fn command_action(&self, key: &str) -> Option<&Arc<Action>> {
        self.action(key)
            .filter(|a| a.command.is_some())
            .or_else(|| {
                self.registered_actions().find(|a| {
                    a.command
                        .as_ref()
                        .is_some_and(|c| c.aliases.iter().any(|alias| alias == key))
                })
            })
    }
    pub(crate) fn registered_actions(&self) -> impl Iterator<Item = &Arc<Action>> {
        self.actions.values()
    }
    pub(crate) fn discover_actions(
        &self,
        target: &Target,
    ) -> impl Iterator<Item = &Arc<Action>> + use<'_> {
        // At most 256 entries inspected: 8 per target, or 8 commands plus 8
        // ordinary actions for Empty. Canonical key order
        // defines precedence independently of extension registration order.
        self.actions.discover(target)
    }
    pub(crate) fn inventory_action(&self, state: BlockStateId) -> Option<&Arc<Action>> {
        let key = &self.block_type(self.state(state)?.block_type)?.key;
        self.discover_actions(&Target::Block(key.to_string()))
            .find(|a| a.operation == Operation::Inventory)
    }
}
