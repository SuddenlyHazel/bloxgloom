//! Exact automation selection. Public behaviors name slots or compare to an own
//! slot; only the host inspects components. Selection never falls back to ID-only
//! withdrawal, so an incompatible earlier variant cannot starve a later fit.
use super::super::transfer::PortRoute;
use super::*;

impl Adapter {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn transfer(
        &self,
        snapshot: &EntitySnapshot,
        neighbours: &EntityView,
        offset: [i32; 3],
        own_port: &str,
        peer_port: Option<&str>,
        push: bool,
        selection: &api::TransferSelection,
    ) -> Result<Option<EntityItemTransfer>, EntityError> {
        if !api::FACES.contains(&offset)
            || !(1..=128).contains(&selection.count)
            || selection.source_slot.is_some_and(|i| i >= 54)
            || selection.destination_slot.is_some_and(|i| i >= 54)
        {
            return Err(EntityError::InvalidPayload);
        }
        let own_slots = neighbours
            .automation_slots(snapshot.id)
            .ok_or(EntityError::InvalidPayload)?;
        let reference = match selection.stack {
            api::StackSelector::SameAsSlot(i) => Some(
                own_slots
                    .get(i as usize)
                    .ok_or(EntityError::InvalidPayload)?
                    .as_ref(),
            ),
            _ => None,
        };
        let anchor = snapshot.anchor().ok_or(EntityError::WrongOwnership)?;
        let from = [anchor.x, anchor.y, anchor.z];
        let to = std::array::from_fn(|i| from[i].saturating_add(offset[i]));
        let peer_cell = CellCoord::new(to[0], to[1], to[2]);
        let own_index = self
            .definition
            .ports
            .iter()
            .position(|p| p.name == own_port)
            .ok_or(EntityError::InvalidPayload)? as u8;
        let Some(own) = EntityTransferPolicy::port(self, own_index, offset) else {
            return Ok(None);
        };
        for peer in neighbours.iter().filter(|e| matches!(&e.location, EntityLocation::Anchored {footprint, ..} if footprint.contains(&peer_cell))) {
            for (peer_index, name) in neighbours.ports(peer.id).iter().enumerate() {
                if peer_port.is_some_and(|wanted| wanted != name) { continue; }
                let Some(port) = neighbours.port(peer.id, peer_index as u8, offset.map(|v| -v)) else { continue; };
                let Some(peer_slots) = neighbours.automation_slots(peer.id) else { continue; };
                let (source, destination, source_view, destination_view) = if push {
                    (&own, &port, own_slots, peer_slots)
                } else {
                    (&port, &own, peer_slots, own_slots)
                };
                let destination = if let Some(slot) = selection.destination_slot {
                    let Some(port) = destination.at_slot(slot) else { continue; };
                    port
                } else { destination.clone() };
                for (slot, mut stack) in source.automation_offers(source_view) {
                    if selection.source_slot.is_some_and(|wanted| wanted != slot) || stack.count < selection.count { continue; }
                    let matches = match &selection.stack {
                        api::StackSelector::Any => true,
                        api::StackSelector::Item(key) => self.catalog.item(stack.item).is_some_and(|i| i.key == *key),
                        api::StackSelector::SameAsSlot(_) => reference.flatten().is_some_and(|s| s.key == stack.key),
                    };
                    if !matches { continue; }
                    stack.count = selection.count;
                    if destination.automation_accepts(destination_view, stack) {
                        return Ok(Some(EntityItemTransfer {
                            source: peer.id, push, item: stack.item, count: stack.count,
                            route: Some(PortRoute {
                                source: if push { own_index } else { peer_index as u8 },
                                destination: if push { peer_index as u8 } else { own_index },
                                source_slot: slot,
                                destination_slot: selection.destination_slot,
                                from: if push { from } else { to },
                                to: if push { to } else { from },
                            }),
                        }));
                    }
                }
            }
        }
        Ok(None)
    }
}
