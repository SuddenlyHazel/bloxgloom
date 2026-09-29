use super::*;
use crate::render::AvatarModel;

fn avatar(id: u64) -> VisualAvatar {
    VisualAvatar {
        animation: Default::default(),
        model: AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE),
        pose: [0.0; 4],
        airborne: false,
        id,
        position: Vec3::new(2.5, 80.0, 0.5),
        cosmetics: [0; 4],
        light_levels: [0; 4],
        bounce: [0; 4],
    }
}

#[test]
fn embers_follow_only_presented_entities_expire_and_stay_bounded() {
    let mut effects = EffectBuffer::default();
    for _ in 0..40 {
        effects.push(7, [0.0, 0.5, 0.0]);
    }
    assert_eq!(effects.embers.len(), MAX_EMBERS);
    let now = Instant::now();
    assert!(effects.visuals(now, &[avatar(8)]).is_empty());
    let shown = effects.visuals(now, &[avatar(7)]);
    assert_eq!(shown.len(), MAX_EMBERS);
    assert_eq!(shown[0].center, Vec3::new(2.5, 80.5, 0.5));
    assert!(effects.visuals(now + LIFE, &[avatar(7)]).is_empty());
    effects.clear();
    assert!(effects.visuals(now, &[avatar(7)]).is_empty());
}
