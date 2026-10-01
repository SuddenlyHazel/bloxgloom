use super::*;

#[test]
fn replica_parameters_use_public_data_and_reset_with_the_session() {
    use crate::render::parameters::{Definition, State, Value};
    let mut parameters = State::default();
    let definition: Definition =
        serde_json::from_str(r#"{"name":"gain","kind":"float","default":0,"min":0,"max":1}"#)
            .unwrap();
    parameters.register("demo:surface", &[definition]).unwrap();
    let script = Arc::new(Script {
        module: "demo@1:visuals".into(),
        source: "return function(input) local e=input.entities[1]; return {{op='parameter',resource='demo:surface',name='gain',value=e and #e.public/10 or 0}} end".into(),
    });
    let mut session =
        VisualSession::with_parameters(Arc::clone(&script), parameters.clone()).unwrap();
    session.entities(vec![view(1, false)], 1);
    session.wait_for_test().unwrap();
    let updates = session.take_parameters();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].value, Value::Scalar(0.6));
    let mut replacement = VisualSession::with_parameters(script, parameters).unwrap();
    assert!(replacement.take_parameters().is_empty());
    replacement.entities(vec![], 0);
    replacement.wait_for_test().unwrap();
    assert_eq!(replacement.take_parameters()[0].value, Value::Scalar(0.0));
}

#[test]
fn invalid_parameter_prevents_other_replica_mutations() {
    let script = Arc::new(Script {
        module: "demo@1:visuals".into(),
        source: "return function(input) local e=input.entities[1]; return {{op='visual',id_lo=e.id_lo,id_hi=e.id_hi,yaw=0.5,bob=0,squash=0},{op='parameter',resource='other:surface',name='gain',value=1}} end".into(),
    });
    let mut session = VisualSession::new(script).unwrap();
    session.entities(vec![view(1, false)], 1);
    assert!(
        session
            .wait_for_test()
            .unwrap_err()
            .contains("foreign visual parameter")
    );
    assert_eq!(session.visual_pose(1), None);
    assert!(session.take_parameters().is_empty());
    assert!(matches!(
        session.worker.replies.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
}

fn view(id: u64, anchored: bool) -> EntityView {
    EntityView {
        id,
        key: "demo:thing".into(),
        position: [2.5, 80.5, 3.5],
        revision: 7,
        motion_revision: if anchored { 0 } else { 9 },
        public: b"public".to_vec(),
    }
}

#[test]
fn mobile_and_anchor_batches_both_reach_worker_without_losing_pose() {
    let script = Arc::new(Script {
        module: "demo@1:visuals".into(),
        source: "return function(input) local e=input.entities[1]; if not e then return {} end if input.event=='replica:entities' then return {{op='visual',id_lo=e.id_lo,id_hi=e.id_hi,yaw=0.2,bob=0,squash=0}} end if input.event=='replica:anchors' then return {{op='spark',id_lo=e.id_lo,id_hi=e.id_hi,x=0,y=0.6,z=0,r=0.2,g=0.8,b=1}} end return {} end".into(),
    });
    let mut session = VisualSession::new(script).unwrap();
    session.anchors(vec![], 0);
    assert_eq!(session.sequence, 0);
    session.entities(vec![view(1, false)], 1);
    session.anchors(vec![view(2, true)], 1);
    session.wait_for_test().unwrap();
    session.poll();
    session.wait_for_test().unwrap();
    assert_eq!(session.visual_pose(1), Some([0.2, 0.0, 0.0]));
    let effects = session.effects(std::time::Instant::now(), &[]);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].center, glam::Vec3::new(2.5, 81.1, 3.5));
    session.anchors(vec![], 0);
    session.wait_for_test().unwrap();
    assert!(session.effects(std::time::Instant::now(), &[]).is_empty());
    assert_eq!(session.visual_pose(1), Some([0.2, 0.0, 0.0]));
}

#[test]
fn anchor_batch_cannot_set_mobile_pose() {
    let script = Arc::new(Script {
        module: "demo@1:visuals".into(),
        source: "return function(input) local e=input.entities[1]; return {{op='visual',id_lo=e.id_lo,id_hi=e.id_hi,yaw=0,bob=0,squash=0}} end".into(),
    });
    let mut session = VisualSession::new(script).unwrap();
    session.anchors(vec![view(2, true)], 1);
    assert!(
        session
            .wait_for_test()
            .unwrap_err()
            .contains("invalid visual replica command")
    );
    assert_eq!(session.visual_pose(2), None);
}

#[test]
fn typed_observations_coalesce_and_remain_available_to_ui_less_entity_callbacks() {
    let script = Arc::new(Script {
        module: "demo@1:visuals".into(),
        source: r#"
        return function(e)
            assert(#e.replica.inventory.slots==36)
            assert(#e.replica.actions==1 and e.replica.actions[1].key=='demo:use')
            if e.event=='replica:inventory' then
                assert(e.replica.inventory.revision_lo==tonumber(e.value))
                return {}
            end
            assert(e.event=='replica:entities' and e.replica.inventory.revision_lo==11)
            assert(#e.entered==1 and #e.left==0)
            return {{op='visual',id_lo=e.entities[1].id_lo,id_hi=e.entities[1].id_hi,yaw=0.1,bob=0,squash=0}}
        end
    "#
        .into(),
    });
    let snapshot = |revision| {
        Arc::new(Observations {
            inventory: Some(super::super::InventoryView {
                revision,
                slots: (0..36)
                    .map(|slot| super::super::SlotView { slot, stack: None })
                    .collect(),
            }),
            actions: vec![
                super::super::ActionView {
                    id: (1u128 << 64) | 1,
                    key: Some("demo:use".into()),
                    accepted: true,
                    reason: String::new(),
                },
                super::super::ActionView {
                    id: (1u128 << 64) | 2,
                    key: Some("other:use".into()),
                    accepted: true,
                    reason: String::new(),
                },
            ],
            ..Default::default()
        })
    };
    let mut session = VisualSession::new(script).unwrap();
    session.enable_observation_events();
    session.observe("replica:inventory", "7".into(), snapshot(7));
    session.observe("replica:inventory", "9".into(), snapshot(9));
    session.observe("replica:inventory", "11".into(), snapshot(11));
    session.entities(vec![view(1, false)], 1);
    assert_eq!(session.queued.len(), 2);
    session.wait_for_test().unwrap();
    session.poll();
    session.wait_for_test().unwrap();
    session.poll();
    session.wait_for_test().unwrap();
    assert_eq!(session.sequence, 3);
    assert_eq!(session.visual_pose(1), Some([0.1, 0.0, 0.0]));
}
