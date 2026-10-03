use super::*;
#[test]
fn compact_membership_covers_all_256_regions_and_half_open_endpoints() {
    let declarations = (0..256)
        .map(|index| Registration {
            key: format!("demo:r{index}"),
            minimum: [index as f32, 0., 0.],
            maximum: [index as f32 + 1., 1., 1.],
            service: "demo:players".into(),
        })
        .collect::<Vec<_>>();
    let regions = declarations.iter().collect::<Vec<_>>();
    let mut player = Player {
        profile: 1,
        session: 1,
        entity: 0,
        name: "Rain".into(),
        position: [255.5, 0., 0.],
        appearance: [0; 4],
        model: None,
        model_visual: None,
    };
    assert_eq!(membership(&regions, Some(&player)), [0, 0, 0, 1 << 63]);
    player.position = [64., 0., 0.];
    assert_eq!(membership(&regions, Some(&player)), [0, 1, 0, 0]);
    assert_eq!(membership(&regions, None), [0; 4]);
    player.position = [256., 0., 0.];
    assert_eq!(membership(&regions, Some(&player)), [0; 4]);
}
