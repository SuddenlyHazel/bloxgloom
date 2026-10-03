use super::super::locomotion::tests::view;
use super::*;
use api::Behavior;
struct Invalid(u8);
impl Behavior for Invalid {
    fn initial(&self) -> api::Payload {
        api::Payload::new(())
    }
    fn encode(&self, state: &api::Payload) -> Result<Vec<u8>, api::Error> {
        state
            .downcast_ref::<()>()
            .map(|_| vec![0])
            .ok_or(api::Error::InvalidState)
    }
    fn decode(&self, _: &[u8]) -> Result<api::Payload, api::Error> {
        Ok(self.initial())
    }
    fn public(&self, _: &api::Payload) -> Result<Vec<u8>, api::Error> {
        Ok(vec![0])
    }
    fn pose(&self, _: &[u8]) -> Result<api::Pose, api::Error> {
        Ok(api::Pose {
            yaw: 0.0,
            grounded: true,
        })
    }
    fn tick(&self, c: &api::Context<'_>) -> Result<api::Plan, api::Error> {
        if self.0 == 1 {
            let _ = c.world.solid([10000, 80, 10000]);
        }
        Ok(api::Plan {
            state: (self.0 == 2).then(|| api::Payload::new(true)),
            next_tick: Some(c.tick + 1),
            position: (self.0 == 0).then_some([c.position[0] + 0.1, c.position[1], c.position[2]]),
            lifecycle: Default::default(),
        })
    }
}
#[test]
fn host_rejects_ignored_out_of_range_reads_unchecked_motion_and_invalid_state() {
    let catalog = Catalog::builtins();
    let terrain = view(&[], &[]);
    for invalid_case in 0..3 {
        let mut definition = crate::content::creatures::mossbun::definition();
        definition.behavior = Arc::new(Invalid(invalid_case));
        let adapter = Adapter(Arc::new(definition));
        let location = EntityLocation::Mobile {
            position: [8.5, 80.0, 8.5],
        };
        let snapshot = EntitySnapshot {
            id: EntityId::new(1).unwrap(),
            entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
            revision: 1,
            motion_revision: 1,
            owner: location.owner().unwrap(),
            location,
            private_payload: api::Payload::new(()),
            next_tick: Some(1),
        };
        let result = EntityTickPolicy::plan(
            &adapter,
            &snapshot,
            1,
            &catalog,
            &terrain,
            &EntityView::assemble(vec![], snapshot.id),
        );
        assert!(matches!(
            result,
            Err(EntityError::InvalidLocation
                | EntityError::ViewOutOfRange
                | EntityError::InvalidPayload)
        ));
    }
}

#[test]
fn scripted_creature_support_loss_falls_at_physics_cadence_and_lands_exactly() {
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glb-creatures/packages");
    let catalog = crate::server::startup::ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap()
        .into_preview_catalog();
    let kind = catalog.entity_type_id_by_key("sprout:sproutling").unwrap();
    let definition = catalog.mobile_entity(kind).unwrap();
    let adapter = Adapter(Arc::clone(definition));
    let flat = view(&[(8, 84, 8)], &[]);
    let hole = view(&[], &[]);
    let location = EntityLocation::Mobile {
        position: [8.5, 85.0, 8.5],
    };
    let mut snapshot = EntitySnapshot {
        id: EntityId::new(7).unwrap(),
        entity_type: kind,
        revision: 1,
        motion_revision: 1,
        owner: location.owner().unwrap(),
        location,
        private_payload: definition.behavior.initial(),
        next_tick: Some(100),
    };
    let plan = |snapshot: &EntitySnapshot, tick, terrain: &VoxelView| {
        EntityTickPolicy::plan(
            &adapter,
            snapshot,
            tick,
            &catalog,
            terrain,
            &EntityView::assemble(vec![], snapshot.id),
        )
        .unwrap()
    };
    let harmless = plan(&snapshot, 10, &flat);
    assert!(harmless.payload.is_none() && harmless.position.is_none());
    assert_eq!(harmless.next_tick, Some(100));

    let mut tick = 10;
    let mut previous_drop = 0.0;
    let mut landed = false;
    for step in 0..30 {
        let EntityLocation::Mobile { position: before } = snapshot.location else {
            panic!("creature must remain mobile");
        };
        let falling = plan(&snapshot, tick, &hole);
        let after = falling.position.unwrap_or(before);
        snapshot.location = EntityLocation::Mobile { position: after };
        snapshot.owner = snapshot.location.owner().unwrap();
        // Roundtrip the real durable codec between steps, including velocity.
        snapshot.private_payload = adapter
            .decode(&adapter.encode(falling.payload.as_ref().unwrap()).unwrap())
            .unwrap();
        snapshot.next_tick = falling.next_tick;
        let pose = definition
            .behavior
            .pose(&adapter.public_view(&snapshot.private_payload).unwrap())
            .unwrap();
        if pose.grounded {
            assert_eq!(after[1], 80.0, "sweep must land on the voxel top");
            assert_eq!(falling.next_tick, Some(tick + 10));
            assert!(
                tick - 10 < 50,
                "five-block fall must finish within a second"
            );
            landed = true;
            break;
        }
        assert_eq!(falling.next_tick, Some(tick + 1));
        let drop = before[1] - after[1];
        assert!(drop > previous_drop, "gravity must accelerate each step");
        previous_drop = drop;
        if step == 0 {
            assert!((drop - 0.032).abs() < 0.00001);
        }
        let early_hint = plan(&snapshot, tick, &hole);
        assert!(early_hint.payload.is_none() && early_hint.position.is_none());
        assert_eq!(early_hint.next_tick, falling.next_tick);
        // Ordinary due work is queued first, then planned at the next barrier.
        tick += 2;
    }
    assert!(landed, "script's ten-tick delay must not slow gravity");
}
