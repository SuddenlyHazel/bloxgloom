use super::*;

#[test]
fn arguments_bound_memory_and_query_slots_before_gpu_setup() {
    for (frames, count, hair) in [
        (0, 1, None),
        (2001, 1, None),
        (1, 0, None),
        (1, 513, None),
        (1, 1, Some(14)),
    ] {
        assert!(validate(frames, count, hair).is_err());
    }
    assert!(validate(1, 1, None).is_ok());
    for hair in 0..=13 {
        assert!(validate(2000, 512, Some(hair)).is_ok());
    }
}

#[test]
fn deterministic_grid_keeps_all_actor_centers_visible_at_extremes() {
    for count in [1, 2, 17, 128, 512] {
        let (actors, matrix) = scene(count, Some(5));
        let (again, second_matrix) = scene(count, Some(5));
        assert_eq!(matrix, second_matrix);
        assert_eq!(actors.len(), count);
        for (index, (actor, other)) in actors.iter().zip(again).enumerate() {
            assert_eq!(actor.id, index as u64 + 1);
            assert_eq!(actor.position, other.position);
            assert_eq!(actor.character_recipe, other.character_recipe);
            assert_eq!(actor.character_recipe.unwrap().hair, 5);
            for y in [0.0, 1.0, 2.1] {
                let clip = matrix * (actor.position + Vec3::Y * y).extend(1.0);
                let ndc = clip.truncate() / clip.w;
                assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
                assert!((0.0..=1.0).contains(&ndc.z));
            }
        }
    }
    let (classic, _) = scene(512, None);
    assert!(classic.iter().all(|actor| actor.character_recipe.is_none()));
}
