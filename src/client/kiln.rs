//! Workstation controls over replicated entity state and durable interactions.
use super::*;
use crate::protocol::kiln::KilnView;

impl ClientApp {
    pub(super) fn open_aimed_kiln(&mut self) -> bool {
        let Some(hit) = self.aimed_block() else {
            return false;
        };
        if !entities::kiln::is_kiln_hit(hit, &self.catalog) {
            return false;
        }
        let Some(entity) = self.replicas.kiln_at(hit.block) else {
            self.show_status("Kiln state is still loading");
            return true;
        };
        let id = entity.id;
        self.set_screen(UiScreen::Kiln);
        self.kiln_target = Some((hit.block, id));
        true
    }

    fn current_kiln(&self) -> Option<&crate::protocol::PublicEntity> {
        let (target, id) = self.kiln_target?;
        self.replicas
            .kiln_at(target)
            .filter(|entity| entity.id == id)
    }

    pub(super) fn kiln_view(&self) -> Option<KilnView> {
        KilnView::decode(&self.current_kiln()?.payload)
    }

    pub(super) fn validate_kiln_screen(&mut self) {
        if self.screen != UiScreen::Kiln {
            return;
        }
        let valid = self.kiln_target.is_some_and(|(cell, _)| {
            self.camera()
                .position
                .distance(Vec3::from_array(cell.map(|v| v as f32 + 0.5)))
                < 8.0
        }) && self.current_kiln().is_some()
            && !self.disconnected;
        if !valid {
            self.set_screen(UiScreen::Playing);
            self.show_status("Kiln is no longer available");
        }
    }

    pub(super) fn kiln_click(&mut self, slot: u8, one: bool) {
        if slot >= 3 {
            return;
        }
        if let Some(inventory) = self.inventory_source {
            if slot == 2 {
                self.show_status("Output is collection only");
                return;
            }
            let count = self.inventory.slots[inventory as usize]
                .as_ref()
                .map_or(0, |s| s.count);
            let free = self.kiln_view().map_or(0, |v| {
                crate::inventory::STACK_LIMIT
                    - v.slots[slot as usize].as_ref().map_or(0, |s| s.count)
            });
            self.kiln_transfer(
                0,
                slot,
                inventory,
                if one {
                    count.min(free).min(1)
                } else {
                    count.min(free)
                },
            );
        } else {
            self.kiln_source = if self.kiln_source == Some(slot) {
                None
            } else {
                Some(slot)
            };
        }
    }

    pub(super) fn kiln_inventory_click(&mut self, slot: u8, one: bool) {
        if usize::from(slot) >= crate::inventory::SLOTS {
            return;
        }
        if let Some(source) = self.kiln_source {
            let count = self
                .kiln_view()
                .and_then(|v| v.slots[source as usize].as_ref().map(|s| s.count))
                .unwrap_or(0);
            let free = crate::inventory::STACK_LIMIT
                - self.inventory.slots[slot as usize]
                    .as_ref()
                    .map_or(0, |s| s.count);
            self.kiln_transfer(
                1,
                source,
                slot,
                if one {
                    count.min(free).min(1)
                } else {
                    count.min(free)
                },
            );
        } else {
            self.inventory_source = if self.inventory_source == Some(slot) {
                None
            } else {
                Some(slot)
            };
        }
    }

    fn kiln_transfer(&mut self, operation: u8, kiln_slot: u8, inventory_slot: u8, count: u16) {
        if count == 0 {
            self.show_status("Source empty or destination full");
            return;
        }
        let Some(entity) = self.current_kiln() else {
            return;
        };
        let mut payload = vec![2, operation, kiln_slot, inventory_slot];
        payload.extend(count.to_le_bytes());
        payload.extend(entity.id.to_le_bytes());
        payload.extend(entity.revision.to_le_bytes());
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        let target = self.kiln_target.unwrap().0;
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target,
            payload,
        });
        self.inventory_source = None;
        self.kiln_source = None;
    }
}
