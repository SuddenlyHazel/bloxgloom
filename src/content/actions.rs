//! Frozen action index shared by discovery and authoritative dispatch.
use super::*;
use bloxgloom_host_api::{RegistrationError as Error, actions::*};
use std::sync::Arc;

impl Catalog {
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
    pub(crate) fn registered_actions(&self) -> impl Iterator<Item = &Arc<Action>> {
        self.actions.values()
    }
    pub(crate) fn discover_actions(
        &self,
        target: &Target,
    ) -> impl Iterator<Item = &Arc<Action>> + use<'_> {
        // At most 256 entries inspected, at most 8 returned. Canonical key order
        // defines precedence independently of extension registration order.
        self.actions.discover(target)
    }
    pub(crate) fn inventory_action(&self, state: BlockStateId) -> Option<&Arc<Action>> {
        let key = &self.block_type(self.state(state)?.block_type)?.key;
        self.discover_actions(&Target::Block(key.to_string()))
            .find(|a| a.operation == Operation::Inventory)
    }
}
