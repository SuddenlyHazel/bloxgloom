use super::Catalog;
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::{EventKind, HandlerRegistration},
};
use std::sync::Arc;

impl Catalog {
    pub(crate) fn register_gameplay_handler(
        &mut self,
        handler: HandlerRegistration,
    ) -> Result<(), RegistrationError> {
        handler.validate()?;
        if handler
            .target
            .as_ref()
            .is_some_and(|target| match handler.event {
                EventKind::BlockPlaced | EventKind::BlockRemoved | EventKind::NeighborChanged => {
                    self.block_by_key(target).is_none()
                }
                EventKind::ActionRequested => self.action(target).is_none_or(|action| {
                    action.operation != bloxgloom_host_api::actions::Operation::Gameplay
                }),
                EventKind::EntityTick => self.gameplay_entity(target).is_none(),
                EventKind::MovingTick | EventKind::MovingImpact | EventKind::MovingExpiry => self
                    .entity_type_id_by_key(target)
                    .is_none_or(|id| self.moving_entity(id).is_none()),
                // Automatic pickup currently selects the stock world-drop
                // inventory, not an arbitrary nearby entity type.
                EventKind::PickupRequested => target != "bloxgloom:drop",
            })
            || (matches!(
                handler.event,
                EventKind::ActionRequested
                    | EventKind::EntityTick
                    | EventKind::MovingTick
                    | EventKind::MovingImpact
                    | EventKind::MovingExpiry
            ) && handler.target.is_none())
        {
            return Err(RegistrationError(format!(
                "{}: unknown gameplay target",
                handler.key
            )));
        }
        if self.gameplay_handlers.len() >= 4096
            || self.gameplay_handlers.values().any(|old| {
                old.key == handler.key
                    || (old.event == handler.event && old.target == handler.target)
            })
        {
            return Err(RegistrationError(format!(
                "{}: duplicate gameplay key/decision owner or handler limit",
                handler.key
            )));
        }
        let id = self
            .gameplay_handlers
            .keys()
            .next_back()
            .map_or(0, |id| id + 1);
        let handler = Arc::new(handler);
        self.gameplay_dispatch
            .entry(handler.event)
            .or_default()
            .insert(
                handler.target.clone().unwrap_or_default(),
                Arc::clone(&handler),
            );
        self.gameplay_handlers.insert(id, handler);
        Ok(())
    }

    pub(crate) fn gameplay_handler(
        &self,
        event: EventKind,
        target: &str,
    ) -> Option<&Arc<HandlerRegistration>> {
        let handlers = self.gameplay_dispatch.get(&event)?;
        handlers.get(target).or_else(|| handlers.get(""))
    }

    pub(crate) fn has_targeted_neighbor_handlers(&self) -> bool {
        self.gameplay_dispatch
            .get(&EventKind::NeighborChanged)
            .is_some_and(|handlers| handlers.keys().any(|key| !key.is_empty()))
    }
}
