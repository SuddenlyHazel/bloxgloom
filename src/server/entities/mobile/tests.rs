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
