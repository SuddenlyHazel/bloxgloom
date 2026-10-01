use super::*;
use crate::render::AvatarModel;

fn avatar(id: u64) -> VisualAvatar {
    VisualAvatar {
        character_pose: [0.0; 3],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
        animation: Default::default(),
        model: AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE),
        pose: [0.0; 4],
        airborne: false,
        id,
        position: Vec3::new(2.5, 80.0, 0.5),
        cosmetics: [0; 4],
        light_levels: [0; 4],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}

#[test]
fn embers_follow_only_presented_entities_expire_and_stay_bounded() {
    let mut effects = EffectBuffer::default();
    for _ in 0..40 {
        effects.push(7, [0.0, 0.5, 0.0]);
    }
    assert_eq!(effects.entries.len(), MAX_EMBERS);
    let now = Instant::now();
    assert!(
        effects
            .visuals(now, &[avatar(8)], &BTreeMap::new())
            .is_empty()
    );
    let shown = effects.visuals(now, &[avatar(7)], &BTreeMap::new());
    assert_eq!(shown.len(), MAX_EMBERS);
    assert_eq!(shown[0].center, Vec3::new(2.5, 80.5, 0.5));
    assert!(
        effects
            .visuals(now + LIFE, &[avatar(7)], &BTreeMap::new())
            .is_empty()
    );
    effects.clear();
    assert!(
        effects
            .visuals(now, &[avatar(7)], &BTreeMap::new())
            .is_empty()
    );
}

#[test]
fn colored_sparks_follow_the_same_bounded_session_lifetime() {
    let mut effects = EffectBuffer::default();
    effects.spark_with(7, [0.0, 0.5, 0.0], [0.2, 0.8, 1.0], 0.16, 850);
    let now = Instant::now();
    assert!(
        effects
            .visuals(now, &[avatar(8)], &BTreeMap::new())
            .is_empty()
    );
    let shown = effects.visuals(now, &[avatar(7)], &BTreeMap::new());
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].style, FireStyle::Spark([0.2, 0.8, 1.0], 0.16));
    assert!(
        effects
            .visuals(now + LIFE, &[avatar(7)], &BTreeMap::new())
            .is_empty()
    );
}

#[test]
fn anchor_effects_use_only_current_offered_positions() {
    let mut effects = EffectBuffer::default();
    effects.spark_with(91, [0.0, 0.6, 0.0], [0.2, 0.8, 1.0], 0.16, 850);
    let now = Instant::now();
    assert!(effects.visuals(now, &[], &BTreeMap::new()).is_empty());
    let anchors = BTreeMap::from([(91, [2.5, 80.5, 3.5])]);
    let shown = effects.visuals(now, &[], &anchors);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].center, Vec3::new(2.5, 81.1, 3.5));
    assert!(effects.visuals(now + LIFE, &[], &anchors).is_empty());
}

#[test]
fn custom_sparks_keep_their_own_lifetimes_and_sizes() {
    let mut effects = EffectBuffer::default();
    effects.spark_with(91, [0.0, 0.6, 0.0], [0.2, 0.8, 1.0], 0.28, 1200);
    let now = Instant::now();
    let anchors = BTreeMap::from([(91, [2.5, 80.5, 3.5])]);
    let shown = effects.visuals(now + LIFE, &[], &anchors);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].style, FireStyle::Spark([0.2, 0.8, 1.0], 0.28));
    assert!(
        effects
            .visuals(now + Duration::from_millis(1200), &[], &anchors)
            .is_empty()
    );
}
