use super::*;
use crate::gameplay::*;
struct World;
impl Snapshot for World {
    fn tick(&self) -> u64 {
        100
    }
    fn seed(&self) -> u64 {
        1
    }
    fn player(&self) -> Option<u128> {
        Some(7)
    }
    fn player_authority(&self, ns: &str) -> bool {
        ns == "demo"
    }
    fn players(&mut self) -> Result<Vec<Player>, Error> {
        Ok(vec![Player {
            profile: 7,
            session: 9,
            entity: 1,
            name: "Player".into(),
            position: [0.0; 3],
            appearance: [0; 4],
            model: None,
            model_visual: None,
        }])
    }
    fn player_model_schema(&self, key: &str) -> Option<VisualSchema> {
        (key == "demo:rig").then(|| VisualSchema {
            clips: vec!["wave".into()],
            clip_loops: vec![false],
            layers: vec!["hat".into()],
            ..Default::default()
        })
    }
    fn entity(&mut self, _: u64) -> Result<Option<Entity>, Error> {
        Ok(None)
    }
    fn nearby_entities(&mut self, _: [f32; 3], _: f32) -> Result<Vec<Entity>, Error> {
        Ok(vec![])
    }
    fn entity_state(&mut self, _: u64, _: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn project_entity_state(&self, _: u64, _: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }
    fn validate_entity_state(&self, _: &str, _: &str, _: &[u8]) -> Result<(), Error> {
        Ok(())
    }
    fn anchored_entity_at(&mut self, _: Cell) -> Result<Option<u64>, Error> {
        Ok(None)
    }
    fn block(&mut self, _: Cell) -> Result<Block, Error> {
        Err(Error::Host("unused".into()))
    }
    fn state(&self, _: &str) -> Result<Block, Error> {
        Err(Error::Host("unused".into()))
    }
    fn item_exists(&self, _: &str) -> bool {
        false
    }
    fn inventory(&mut self, _: InventoryId) -> Result<Vec<Slot>, Error> {
        Ok(vec![])
    }
    fn validate_stack(&self, _: &Stack) -> Result<(), Error> {
        Ok(())
    }
    fn inventory_accepts(&self, _: InventoryId, _: usize, _: &Stack) -> bool {
        false
    }
}
#[test]
fn staged_player_models_and_clips_overlay_directory_and_stop_without_changing_look() {
    let mut world = World;
    let mut context = Context::new(&mut world, 128);
    context.handler_namespace = Some("demo".into());
    context.set_player_model(7, 9, Some("demo:rig")).unwrap();
    let (key, mut visual) = context.player_model(7, 9).unwrap().unwrap();
    assert_eq!(key, "demo:rig");
    visual.layers[0] = 1;
    context.set_player_model_visual(7, 9, visual).unwrap();
    context
        .play_player_animation(7, 9, "wave", 1.0, true, 0.1)
        .unwrap();
    let visual = context.player_model(7, 9).unwrap().unwrap().1;
    assert!(visual.playback.unwrap().looping);
    assert_eq!(visual.playback.unwrap().started_tick, 100);
    assert_eq!(visual.layers[0], 1);
    context.stop_player_animation(7, 9, 0.15).unwrap();
    let visual = context.player_model(7, 9).unwrap().unwrap().1;
    assert!(visual.playback.is_none());
    assert_eq!(visual.layers[0], 1);
    assert_eq!(visual.transition_s, 0.15);
    assert_eq!(context.plan.player_operations.len(), 4);
}
#[test]
fn bad_model_clip_or_replacement_session_poison_the_whole_plan() {
    for invalid in 0..3 {
        let mut world = World;
        let mut context = Context::new(&mut world, 64);
        context.handler_namespace = Some("demo".into());
        context.set_player_model(7, 9, Some("demo:rig")).unwrap();
        let rejected = match invalid {
            0 => context.set_player_model(7, 9, Some("demo:missing")),
            1 => context.play_player_animation(7, 9, "missing", 1.0, false, 0.2),
            _ => context.stop_player_animation(7, 10, 0.2),
        };
        assert!(rejected.is_err());
        assert!(context.finish().is_err());
    }
}
