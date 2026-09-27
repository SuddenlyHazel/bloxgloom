use super::*;
pub(crate) struct MobileProbe {
    app: ClientApp,
}
impl MobileProbe {
    pub(crate) fn new(catalog: Arc<crate::content::Catalog>, path: PathBuf) -> Self {
        let mut network = Network::disconnected_for_test();
        network.catalog = catalog;
        Self {
            app: ClientApp::new(network, Config::default(), path),
        }
    }
    pub(crate) fn accept(&mut self, message: ServerMessage) {
        self.app.accept(message);
        assert!(!self.app.disconnected);
        assert!(
            !self
                .app
                .pending_commands
                .iter()
                .any(|m| matches!(m, ClientMessage::Resync { .. }))
        );
    }
    pub(crate) fn next_id(&mut self) -> u128 {
        self.app.allocate_action_id().unwrap()
    }
    pub(crate) fn status(&self) -> Option<&str> {
        self.app
            .status
            .as_ref()
            .map(|(message, _)| message.as_str())
    }
    pub(crate) fn item_slot(&self, item: crate::items::ItemId) -> Option<u8> {
        self.app
            .inventory
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|stack| stack.item == item))
            .map(|slot| slot as u8)
    }
    pub(crate) fn entity(
        &self,
        id: crate::content::EntityTypeId,
    ) -> Option<crate::protocol::PublicEntity> {
        self.app.replicas.mobile_for_test(id)
    }
    pub(crate) fn interact(&mut self, entity: &crate::protocol::PublicEntity) -> ClientMessage {
        let crate::protocol::PublicEntityLocation::Mobile { position } = entity.location else {
            panic!("expected mobile")
        };
        let body = self
            .app
            .catalog
            .mobile_entity(entity.entity_type)
            .unwrap()
            .body;
        let direction = (Vec3::from_array(position) + Vec3::Y * (body.height * 0.5)
            - self.app.camera().position)
            .normalize();
        self.app.yaw = direction.z.atan2(direction.x);
        self.app.pitch = direction.y.asin();
        // Use the production ray selection and command builder, not a fixture
        // protocol request. The raw authoritative position drives targeting.
        self.app.pending_commands.clear();
        assert!(self.app.interact_aimed_mobile());
        let request = self.app.pending_commands.pop_front().unwrap();
        assert!(matches!(request, ClientMessage::EntityInteract { .. }));
        request
    }
}
impl Drop for MobileProbe {
    fn drop(&mut self) {
        self.app.config_writer.finish();
    }
}
