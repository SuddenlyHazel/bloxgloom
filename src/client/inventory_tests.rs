//! Headless harness for the actual client inventory discovery and click path.
use super::*;
pub(crate) struct InventoryProbe {
    app: ClientApp,
}
impl InventoryProbe {
    pub(crate) fn new(catalog: Arc<crate::content::Catalog>, path: PathBuf) -> Self {
        let mut network = Network::disconnected_for_test();
        network.catalog = catalog;
        Self {
            app: ClientApp::new(network, Config::default(), path),
        }
    }
    pub(crate) fn accept(&mut self, message: ServerMessage) {
        self.app.accept(message);
        assert!(
            !self.app.disconnected,
            "client rejected valid inventory stream"
        );
        assert!(
            !self
                .app
                .pending_commands
                .iter()
                .any(|m| matches!(m, ClientMessage::Resync { .. })),
            "valid inventory stream required resync"
        );
    }
    pub(crate) fn ready(&self, key: ChunkKey) -> bool {
        self.app.chunks.contains_key(&key)
    }
    pub(crate) fn block_state(&self, at: [i32; 3]) -> Option<crate::content::BlockStateId> {
        self.app.block_at(at[0], at[1], at[2])
    }
    pub(crate) fn next_id(&mut self) -> u128 {
        self.app.allocate_action_id().unwrap()
    }
    pub(crate) fn item_action(&mut self, slot: usize) -> ClientMessage {
        self.app.pending_commands.clear();
        self.app.config.selected_slot = slot;
        assert!(self.app.open_item_actions());
        assert_eq!(self.app.screen, UiScreen::Actions);
        let panel = self.app.action_panel().unwrap();
        let row = panel
            .widgets
            .iter()
            .position(|w| matches!(w, bloxgloom_host_api::actions::Widget::Button { .. }))
            .unwrap() as u8;
        let layout = UiLayout::new(640, 360, 1.0, UiScreen::Actions).with_actions(Some(&panel));
        let rect = layout.rect(UiControl::Action(row)).unwrap();
        assert_eq!(
            layout.hit_test(rect.x + 1.0, rect.y + 1.0),
            Some(UiControl::Action(row))
        );
        self.app.action_control(row);
        assert_eq!(self.app.screen, UiScreen::Playing);
        self.app
            .pending_commands
            .pop_front()
            .expect("registered control emits request")
    }
    pub(crate) fn action_on_block(&mut self, target: [i32; 3]) -> ClientMessage {
        self.app.pending_commands.clear();
        let direction = (Vec3::from_array(target.map(|v| v as f32 + 0.5))
            - self.app.camera().position)
            .normalize();
        self.app.yaw = direction.z.atan2(direction.x);
        self.app.pitch = direction.y.asin();
        assert!(
            self.app.open_aimed_kiln(),
            "registered block actions are discovered from the production ray target"
        );
        assert_eq!(self.app.screen, UiScreen::Actions);
        self.app.action_control(0);
        self.app
            .pending_commands
            .pop_front()
            .expect("block control emits registered request")
    }
    pub(crate) fn anchored(&self, at: [i32; 3]) -> Option<crate::protocol::PublicEntity> {
        self.app.replicas.anchored_for_test(at).cloned()
    }
    pub(crate) fn open(&mut self, at: [i32; 3], state: crate::content::BlockStateId) {
        assert!(self.app.open_inventory_at(at, state));
        assert_eq!(self.app.screen, UiScreen::Container);
        self.app.validate_kiln_screen();
        assert_eq!(self.app.screen, UiScreen::Container);
        let descriptor = self.app.container_screen().unwrap();
        let layout =
            UiLayout::new(640, 360, 1.0, self.app.screen).with_container(Some(&descriptor));
        assert_eq!(
            self.app.focus_order().len(),
            usize::from(descriptor.slots) + crate::inventory::SLOTS
        );
        for slot in 0..descriptor.slots {
            assert!(layout.rect(UiControl::KilnSlot(slot)).is_some());
        }
    }
    pub(crate) fn close(&mut self) {
        self.app.on_escape();
        assert_eq!(self.app.screen, UiScreen::Playing);
        assert!(self.app.kiln_target.is_none());
    }
    pub(crate) fn view(&self) -> Option<crate::protocol::workstation::WorkstationView> {
        self.app.kiln_view()
    }
    pub(crate) fn workstation_revision(&self) -> Option<u64> {
        let (target, id) = self.app.kiln_target?;
        self.app
            .replicas
            .kiln_at(target, &self.app.catalog)
            .filter(|entity| entity.id == id)
            .map(|entity| entity.revision)
    }
    pub(crate) fn player_count(&self, slot: usize) -> u16 {
        self.app.inventory.slots[slot]
            .as_ref()
            .map_or(0, |s| s.count)
    }
    pub(crate) fn transfer(
        &mut self,
        deposit: bool,
        player: u8,
        container: u8,
        one: bool,
    ) -> ClientMessage {
        self.app.pending_commands.clear();
        if deposit {
            self.app.kiln_inventory_click(player, false);
            self.app.kiln_click(container, one);
        } else {
            self.app.kiln_click(container, false);
            self.app.kiln_inventory_click(player, one);
        }
        let request = self
            .app
            .pending_commands
            .pop_front()
            .expect("slot clicks must produce a request");
        assert!(matches!(request, ClientMessage::EntityInteract { .. }));
        request
    }
}
impl Drop for InventoryProbe {
    fn drop(&mut self) {
        self.app.config_writer.finish();
    }
}
