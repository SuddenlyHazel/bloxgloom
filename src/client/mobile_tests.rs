use super::*;
pub(crate) struct MobileProbe {
    app: ClientApp,
    _incoming: std::sync::mpsc::SyncSender<Incoming>,
}

/// Full join and installed-replica path for downloaded visual callbacks.
pub(crate) struct NetworkedVisualProbe {
    app: ClientApp,
}

impl NetworkedVisualProbe {
    pub(crate) fn connect(address: &str, profile: u128, path: PathBuf) -> std::io::Result<Self> {
        let network = Network::connect(address, 1, profile)?;
        let app = ClientApp::new(network, Config::default(), path);
        assert!(
            app.visual_session.is_some(),
            "expected downloaded visual worker"
        );
        Ok(Self { app })
    }

    pub(crate) fn accept_next(&mut self) {
        let message = self
            .app
            .network
            .incoming
            .recv_timeout(Duration::from_secs(10))
            .expect("visual replica deadline");
        match message {
            Incoming::Message(message) => self.app.accept(*message),
            Incoming::Closed(error) => panic!("visual replica closed: {error}"),
        }
        assert!(!self.app.disconnected);
        self.app.visual_session.as_mut().unwrap().poll();
    }

    pub(crate) fn entity(
        &self,
        entity_type: crate::content::EntityTypeId,
    ) -> Option<crate::protocol::PublicEntity> {
        self.app.replicas.mobile_for_test(entity_type)
    }

    pub(crate) fn interact(&mut self, entity: &crate::protocol::PublicEntity) -> bool {
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
        let previous = self.app.pending_actions.keys().copied().collect::<Vec<_>>();
        assert!(self.app.interact_aimed_mobile());
        let action_id = *self
            .app
            .pending_actions
            .keys()
            .find(|id| !previous.contains(id))
            .expect("aimed creature did not create a request");
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.app.pending_actions.contains_key(&action_id) {
            assert!(Instant::now() < deadline, "creature interaction deadline");
            self.accept_next();
        }
        self.app.status.as_ref().map(|status| status.0.as_str()) == Some("Interaction applied")
    }

    pub(crate) fn tint(&mut self, id: u64) -> Option<[f32; 3]> {
        let visual = self.app.visual_session.as_mut().unwrap();
        visual.poll();
        visual.visual_tint(id)
    }

    pub(crate) fn offered(&self) -> (Vec<crate::client::presentation::EntityView>, usize) {
        let visual = self.app.visual_session.as_ref().unwrap();
        self.app
            .replicas
            .presentation_entities(visual.owner(), &self.app.catalog)
    }

    pub(crate) fn offered_anchors(&self) -> (Vec<crate::client::presentation::EntityView>, usize) {
        let visual = self.app.visual_session.as_ref().unwrap();
        self.app
            .replicas
            .presentation_anchors(visual.owner(), &self.app.catalog)
    }

    pub(crate) fn has_anchor_spark(&self, id: u64) -> bool {
        let Some(anchor) = self
            .offered_anchors()
            .0
            .into_iter()
            .find(|anchor| anchor.id == id)
        else {
            return false;
        };
        let center = glam::Vec3::from(anchor.position) + glam::Vec3::Y * 0.6;
        self.app
            .visual_session
            .as_ref()
            .unwrap()
            .effects(std::time::Instant::now(), &[])
            .iter()
            .any(|effect| {
                effect.style == crate::render::fire::FireStyle::Spark([0.25, 0.85, 1.0], 0.28)
                    && effect.center == center
            })
    }

    pub(crate) fn has_spark(&self, id: u64) -> bool {
        let avatar = crate::render::VisualAvatar {
            animation: Default::default(),
            model: crate::render::AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE),
            pose: [0.0; 4],
            airborne: false,
            id,
            position: glam::Vec3::ZERO,
            cosmetics: [0; 4],
            light_levels: [0; 4],
            bounce: [0; 4],
            tint: [1.0; 3],
        };
        self.app
            .visual_session
            .as_ref()
            .unwrap()
            .effects(std::time::Instant::now(), &[avatar])
            .iter()
            .any(|effect| {
                effect.style == crate::render::fire::FireStyle::Spark([0.25, 0.85, 1.0], 0.16)
            })
    }

    pub(crate) fn settle(&mut self) {
        let visual = self.app.visual_session.as_mut().unwrap();
        visual.poll();
        visual.wait_for_test().unwrap();
    }
}

impl Drop for NetworkedVisualProbe {
    fn drop(&mut self) {
        self.app.config_writer.finish();
    }
}
impl MobileProbe {
    pub(crate) fn new(catalog: Arc<crate::content::Catalog>, path: PathBuf) -> Self {
        let (mut network, incoming) = Network::idle_for_test();
        network.catalog = catalog;
        Self {
            app: ClientApp::new(network, Config::default(), path),
            _incoming: incoming,
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
    pub(crate) fn workstation(&self, cell: [i32; 3]) -> Option<crate::protocol::PublicEntity> {
        self.app.replicas.kiln_at(cell, &self.app.catalog).cloned()
    }
    pub(crate) fn has_chunk(&self, key: ChunkKey) -> bool {
        self.app.chunks.contains_key(&key)
    }
    /// Exercise the production submission/mailbox/light/mesh path. Presentation
    /// is separately traced in the window; this probe ends at mesh readiness.
    pub(crate) fn mesh_edit(&mut self, key: ChunkKey, bounced: bool) -> Duration {
        let start = Instant::now();
        self.app.config.bounced_gi = bounced;
        self.app.queue_edited_chunk_relight(key);
        let revision = self.app.lighting_revisions[&key];
        loop {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "edited mesh starved"
            );
            self.app.poll_work();
            if let Ok(result) = self
                .app
                .mesher
                .results
                .recv_timeout(Duration::from_millis(10))
                && result.mesh.key == key
                && result.mesh.lighting_revision == revision
            {
                return start.elapsed();
            }
        }
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
