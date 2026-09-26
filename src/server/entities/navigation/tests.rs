use super::super::locomotion::tests::view;
use super::*;
const BODY: Body = Body {
    half_width: 0.36,
    height: 0.94,
    speed: 1.5625,
};

#[test]
fn astar_routes_around_wall_and_replans_after_terrain_edit() {
    let terrain = view(&[(9, 80, 8)], &[]);
    let mut position = [8.5, 80.0, 8.5];
    let mut detoured = false;
    for _ in 0..10 {
        match route(&terrain, BODY, position, [10, 8]).unwrap() {
            Route::Arrived => break,
            Route::Next(next) => {
                assert!(BODY.walk_edge(&terrain, position, next).unwrap());
                detoured |= next[2] != 8.5;
                position = next;
            }
            other => panic!("expected a route, got {other:?}"),
        }
    }
    assert_eq!(position, [10.5, 80.0, 8.5]);
    assert!(detoured);
    assert_eq!(
        route(&view(&[], &[]), BODY, [8.5, 80.0, 8.5], [10, 8]).unwrap(),
        Route::Next([9.5, 80.0, 8.5])
    );
}

#[test]
fn unreachable_goals_and_missing_terrain_are_explicit() {
    let walls = [(8, 80, 9), (9, 80, 8), (8, 80, 7), (7, 80, 8)];
    assert_eq!(
        route(&view(&walls, &[]), BODY, [8.5, 80.0, 8.5], [10, 8]).unwrap(),
        Route::Unreachable
    );
    assert_eq!(
        route(&view(&[], &[]), BODY, [8.5, 80.0, 8.5], [100, 8]).unwrap(),
        Route::Unreachable
    );
    let empty = VoxelView::from_chunks(Vec::<crate::world::Chunk>::new()).unwrap();
    assert!(matches!(
        route(&empty, BODY, [8.5, 80.0, 8.5], [10, 8]),
        Err(EntityError::ViewOutOfRange)
    ));
}

#[test]
fn blocked_goal_exhausts_bounded_search_without_unbounded_exploration() {
    let terrain = view(&[(9, 80, 8), (11, 80, 8), (10, 80, 7), (10, 80, 9)], &[]);
    assert_eq!(
        route(&terrain, BODY, [8.5, 80.0, 8.5], [10, 8]).unwrap(),
        Route::BudgetExhausted
    );
}
