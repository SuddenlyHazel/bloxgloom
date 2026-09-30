//! Headless real-network harness for authored callback -> current aim -> receipt.
use super::*;

#[test]
fn authored_entity_action_uses_observed_identity_and_exact_bounded_arguments() {
    let catalog = crate::content::Catalog::builtins();
    let action = Action {
        key: "demo:pat".into(),
        version: 3,
        label: "Pat".into(),
        target: Target::Entity("bloxgloom:mossbun".into()),
        operation: Operation::Gameplay,
        panel: None,
        command: None,
    };
    let entity = crate::protocol::PublicEntity {
        id: 91,
        entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
        revision: 7,
        motion_revision: 4,
        location: crate::protocol::PublicEntityLocation::Mobile {
            position: [2.5, 3.0, -1.5],
        },
        payload: vec![0, 1],
    };
    let ClientMessage::EntityInteract {
        target, payload, ..
    } = compose_observed_entity_action(
        &catalog,
        &action,
        &entity,
        2,
        11,
        vec![0, 255],
        1u128 << 64 | 9,
    )
    .unwrap()
    else {
        panic!("expected entity request")
    };
    assert_eq!(target, [2, 3, -2]);
    let request = Request::decode(&payload).unwrap();
    assert_eq!(
        (
            request.entity,
            request.entity_revision,
            request.slot,
            request.inventory_revision
        ),
        (91, 7, 2, 11)
    );
    assert_eq!(request.arguments, [0, 255]);
    let mut wrong = action.clone();
    wrong.target = Target::Entity("demo:other".into());
    assert!(
        compose_observed_entity_action(&catalog, &wrong, &entity, 2, 11, vec![], 1u128 << 64 | 9)
            .is_none()
    );
    assert!(
        compose_observed_entity_action(
            &catalog,
            &action,
            &entity,
            2,
            11,
            vec![0; 131],
            1u128 << 64 | 9
        )
        .is_none()
    );
}

#[test]
fn named_shortcut_is_inert_without_matching_session_command() {
    use bloxgloom_host_api::actions::{Command, CommandArgument, CommandPermission};
    let mut catalog = crate::content::Catalog::builtins();
    let inventory = crate::inventory::Inventory::default();
    assert!(compose_named_command(&catalog, "demo:wave", 0, &inventory, [0; 3]).is_none());
    catalog
        .register_action(Action {
            key: "demo:wave".into(),
            version: 1,
            label: "Wave".into(),
            target: Target::Empty,
            operation: Operation::Gameplay,
            panel: None,
            command: Some(Command {
                permission: CommandPermission::Player,
                arguments: vec![],
            }),
        })
        .unwrap();
    let request = compose_named_command(&catalog, "demo:wave", 0, &inventory, [1, 2, 3]).unwrap();
    let ClientMessage::EntityInteract {
        target, payload, ..
    } = request
    else {
        panic!("expected registered request")
    };
    assert_eq!(target, [1, 2, 3]);
    assert_eq!(Request::decode(&payload).unwrap().key, "demo:wave");
    assert!(compose_named_command(&catalog, "other:wave", 0, &inventory, [0; 3]).is_none());
    catalog
        .register_action(Action {
            key: "demo:target".into(),
            version: 1,
            label: "Target".into(),
            target: Target::Empty,
            operation: Operation::Gameplay,
            panel: None,
            command: Some(Command {
                permission: CommandPermission::Player,
                arguments: vec![CommandArgument::EntityKey { max_bytes: 32 }],
            }),
        })
        .unwrap();
    assert!(compose_named_command(&catalog, "demo:target", 0, &inventory, [0; 3]).is_none());
}

pub(crate) struct PackageActionProbe {
    app: ClientApp,
}

impl PackageActionProbe {
    pub(crate) fn connect_document(address: &str, profile: u128, path: PathBuf) -> Self {
        let network = Network::connect(address, 1, profile).unwrap();
        let mut app = ClientApp::new(network, Config::default(), path);
        app.config.selected_slot = 0;
        app.package_ui.as_mut().unwrap().resize(640, 360, 1.0);
        Self { app }
    }
    pub(crate) fn session_mut(&mut self) -> &mut crate::ui::authored::Session {
        self.app.package_ui.as_mut().unwrap()
    }

    pub(crate) fn select_slot(&mut self, slot: usize) {
        self.app.config.selected_slot = slot;
    }

    pub(crate) fn open_recipe_binding(&mut self) {
        assert!(self.app.package_binding_key(KeyCode::KeyB, false, false));
        assert_eq!(self.app.screen, UiScreen::Package);
        self.session_mut().wait_for_presentation().unwrap();
    }

    pub(crate) fn submit_ui_action(&mut self) -> ClientMessage {
        let id = action_id(self.app.actions.epoch, self.app.actions.next_seq);
        self.app.pump_package_action();
        assert_eq!(self.feedback(), "WAITING FOR SERVER");
        self.app.pending_actions[&id].clone()
    }

    pub(crate) fn resend(&mut self, message: &ClientMessage) {
        assert!(self.app.network.send(message.clone()));
    }

    pub(crate) fn inventory_count(&self, slot: usize) -> u16 {
        self.app.inventory.slots[slot]
            .as_ref()
            .map_or(0, |stack| stack.count)
    }
    pub(crate) fn connect(address: &str, profile: u128, path: PathBuf) -> Self {
        Self::connect_for(address, profile, path, "uitarget:light")
    }

    pub(crate) fn connect_for(address: &str, profile: u128, path: PathBuf, event: &str) -> Self {
        let network = Network::connect(address, 1, profile).unwrap();
        let mut app = ClientApp::new(network, Config::default(), path);
        app.config.selected_slot = 0;
        let ui = app.package_ui.as_mut().unwrap();
        ui.resize(640, 360, 1.0);
        for _ in 0..64 {
            ui.tab(false);
            if ui.event() == Some(event) {
                break;
            }
        }
        assert_eq!(ui.event(), Some(event));
        Self { app }
    }

    pub(crate) fn title(&self) -> &str {
        self.app.package_ui.as_ref().unwrap().text_at(1)
    }

    pub(crate) fn has_downloaded_material_and_startup(&self, package: &str) -> bool {
        self.app.network.bundle_for_test().is_some_and(|bundle| {
            bundle.material().is_some()
                && bundle.packages()[package]
                    .sources
                    .contains_key("client_startup")
                && !bundle.packages()[package].sources.contains_key("plant")
        })
    }

    pub(crate) fn selected_material_layer(&self) -> u32 {
        let bundle = self.app.network.bundle_for_test().unwrap();
        bundle
            .material()
            .unwrap()
            .resolve(&self.app.catalog)
            .unwrap()
            .selected_layer()
    }

    fn read(&mut self, deadline: Instant) -> ServerMessage {
        let incoming = self
            .app
            .network
            .incoming
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("targeted UI stream deadline");
        match incoming {
            Incoming::Message(message) => *message,
            Incoming::Closed(reason) => panic!("targeted UI connection closed: {reason}"),
        }
    }

    pub(crate) fn ready(&mut self, at: [i32; 3], block: BlockId, revision: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.app.actions.epoch == 0
            || self.app.position != Vec3::new(0.5, 80.0, 0.5)
            || self.app.inventory.revision != revision
            || self.app.inventory.slots[0].is_none()
            || self.app.block_at(at[0], at[1], at[2]) != Some(block)
        {
            let message = self.read(deadline);
            self.app.accept(message);
            assert!(!self.app.disconnected);
        }
    }

    fn aim(&mut self, at: [i32; 3]) {
        let direction =
            (Vec3::from_array(at.map(|v| v as f32 + 0.5)) - self.app.camera().position).normalize();
        self.app.yaw = direction.z.atan2(direction.x);
        self.app.pitch = direction.y.asin();
    }

    fn activate(&mut self) {
        let ui = self.app.package_ui.as_mut().unwrap();
        ui.activate();
        ui.wait_for_presentation().unwrap();
    }

    pub(crate) fn reject_locally(&mut self, at: [i32; 3]) {
        self.aim(at);
        let next = self.app.actions.next_seq;
        self.activate();
        self.app.pump_package_action();
        assert_eq!(
            self.app.actions.next_seq, next,
            "local failure consumed sequence"
        );
        assert!(self.app.pending_actions.is_empty());
        assert!(self.feedback().starts_with("ACTION NOT SENT:"));
    }

    pub(crate) fn click(&mut self, before_callback: [i32; 3], current: [i32; 3]) -> ClientMessage {
        self.aim(before_callback);
        self.activate();
        // The callback has completed; a new aim before dispatch must win.
        self.aim(current);
        let id = action_id(self.app.actions.epoch, self.app.actions.next_seq);
        self.app.pump_package_action();
        assert_eq!(self.feedback(), "WAITING FOR SERVER");
        self.app.pending_actions[&id].clone()
    }

    pub(crate) fn observe(&mut self, at: [i32; 3]) -> ClientMessage {
        self.aim(at);
        self.activate();
        let key = self.app.package_ui.as_mut().unwrap().take_action().unwrap();
        self.app.compose_current_package_action(&key).unwrap()
    }

    pub(crate) fn submit_observed(&mut self, original: &ClientMessage) -> ClientMessage {
        let ClientMessage::EntityInteract {
            target, payload, ..
        } = original
        else {
            unreachable!()
        };
        let mut observed = TerrainRequest::decode(payload).unwrap();
        // Isolate the terrain precondition from an unrelated inventory change.
        observed.request.inventory_revision = self.app.inventory.revision;
        let action_id = self.app.allocate_action_id().unwrap();
        self.submit(ClientMessage::EntityInteract {
            action_id,
            target: *target,
            payload: observed.encode().unwrap(),
        })
    }

    pub(crate) fn submit_panel_block(&mut self, target: [i32; 3]) -> ClientMessage {
        self.aim(target);
        let next = self.app.actions.next_seq;
        let action_id = action_id(self.app.actions.epoch, next);
        assert!(self.app.open_aimed_kiln());
        assert_eq!(self.app.screen, UiScreen::Actions);
        self.app.action_control(0);
        assert_eq!(self.app.actions.next_seq, next + 1);
        self.app.pending_actions[&action_id].clone()
    }

    pub(crate) fn edit(&mut self, at: [i32; 3], block: BlockId, slot: u8) -> ClientMessage {
        let action_id = self.app.allocate_action_id().unwrap();
        self.submit(ClientMessage::Edit {
            action_id,
            x: at[0],
            y: at[1],
            z: at[2],
            block,
            slot,
        })
    }

    fn submit(&mut self, message: ClientMessage) -> ClientMessage {
        let action_id = match &message {
            ClientMessage::EntityInteract { action_id, .. }
            | ClientMessage::Edit { action_id, .. } => *action_id,
            _ => unreachable!(),
        };
        self.app
            .package_ui
            .as_mut()
            .unwrap()
            .action_submitted(action_id);
        self.app.queue_command(message.clone());
        message
    }

    pub(crate) fn forged(
        &mut self,
        original: &ClientMessage,
        target: [i32; 3],
        change: impl FnOnce(&mut Request),
    ) -> ClientMessage {
        let ClientMessage::EntityInteract { payload, .. } = original else {
            unreachable!()
        };
        let mut observed = TerrainRequest::decode(payload).unwrap();
        let request = &mut observed.request;
        request.inventory_revision = self.app.inventory.revision;
        change(request);
        let [x, y, z] = target;
        // These forgeries isolate other authorization checks from stale terrain;
        // submit_observed deliberately retains the old version for ABA coverage.
        observed.version = self.app.chunks[&crate::world::world_to_chunk(x, y, z).0].version;
        let action_id = self.app.allocate_action_id().unwrap();
        let message = ClientMessage::EntityInteract {
            action_id,
            target,
            payload: observed.encode().unwrap(),
        };
        self.app
            .package_ui
            .as_mut()
            .unwrap()
            .action_submitted(action_id);
        self.app.queue_command(message.clone());
        message
    }

    pub(crate) fn result(&mut self, request: &ClientMessage) -> (bool, String) {
        let expected = match request {
            ClientMessage::EntityInteract { action_id, .. }
            | ClientMessage::Edit { action_id, .. }
            | ClientMessage::InventoryMove { action_id, .. } => action_id,
            _ => unreachable!(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let message = self.read(deadline);
            let result = if let ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            } = &message
                && action_id == expected
            {
                Some((*accepted, reason.clone()))
            } else {
                None
            };
            self.app.accept(message);
            assert!(!self.app.disconnected);
            if let Some(result) = result {
                return result;
            }
        }
    }

    pub(crate) fn reject_old_session(&mut self, request: &ClientMessage) {
        let ClientMessage::EntityInteract {
            action_id: expected,
            ..
        } = request
        else {
            unreachable!()
        };
        assert_ne!((*expected >> 64) as u64, self.app.actions.epoch);
        assert!(self.app.network.send(request.clone()));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let message = self.read(deadline);
            if let ServerMessage::ActionResult {
                action_id,
                accepted,
                ..
            } = message
                && action_id == *expected
            {
                assert!(!accepted);
                // Deliberately forged old-session traffic isn't an outstanding
                // UI request. Do not inject its result into the fresh tracker.
                break;
            }
            self.app.accept(message);
        }
        assert_eq!(self.app.actions.next_seq, 1);
        assert!(self.app.pending_actions.is_empty());
        assert_eq!(self.app.package_ui.as_ref().unwrap().feedback(), None);
    }

    pub(crate) fn feedback(&self) -> &str {
        self.app.package_ui.as_ref().unwrap().feedback().unwrap()
    }

    pub(crate) fn count(&self) -> u16 {
        self.app.inventory.slots[0]
            .as_ref()
            .map_or(0, |stack| stack.count)
    }

    pub(crate) fn spawn_marker(&mut self) -> ClientMessage {
        let action = self
            .app
            .catalog
            .action(crate::gameplay::admin::SPAWN)
            .unwrap();
        let arguments = action
            .command
            .as_ref()
            .unwrap()
            .encode_arguments(&["uitarget:marker"])
            .unwrap();
        let request = Request {
            key: action.key.clone(),
            version: action.version,
            slot: 0,
            inventory_revision: 0,
            entity: 0,
            entity_revision: 0,
            arguments,
        };
        let action_id = self.app.allocate_action_id().unwrap();
        let target = self.app.position.to_array().map(|v| v.floor() as i32);
        self.submit(ClientMessage::EntityInteract {
            action_id,
            target,
            payload: request.encode().unwrap(),
        })
    }

    pub(crate) fn wait_for_marker(&mut self) -> crate::protocol::PublicEntity {
        let entity_type = self
            .app
            .catalog
            .entity_type_id_by_key("uitarget:marker")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(entity) = self.app.replicas.mobile_for_test(entity_type) {
                return entity;
            }
            let message = self.read(deadline);
            self.app.accept(message);
            assert!(!self.app.disconnected);
        }
    }

    pub(crate) fn click_marker(&mut self, entity: &crate::protocol::PublicEntity) -> ClientMessage {
        let crate::protocol::PublicEntityLocation::Mobile { position } = entity.location else {
            panic!("expected mobile marker")
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
        self.activate();
        let id = action_id(self.app.actions.epoch, self.app.actions.next_seq);
        self.app.pump_package_action();
        assert_eq!(self.feedback(), "WAITING FOR SERVER");
        self.app.pending_actions[&id].clone()
    }

    pub(crate) fn forge_entity_arguments(
        &mut self,
        original: &ClientMessage,
        arguments: Vec<u8>,
    ) -> ClientMessage {
        let ClientMessage::EntityInteract {
            target, payload, ..
        } = original
        else {
            unreachable!()
        };
        let mut request = Request::decode(payload).unwrap();
        request.arguments = arguments;
        request.inventory_revision = self.app.inventory.revision;
        let action_id = self.app.allocate_action_id().unwrap();
        self.submit(ClientMessage::EntityInteract {
            action_id,
            target: *target,
            payload: request.encode().unwrap(),
        })
    }

    pub(crate) fn wait_for_count(&mut self, count: u16) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.count() != count {
            let message = self.read(deadline);
            self.app.accept(message);
            assert!(!self.app.disconnected);
        }
    }
}

#[path = "tests/mixed.rs"]
mod mixed;
