use super::*;
#[test]
fn query_bounds_are_checked_before_enumeration() {
    assert!(box_cells([0; 3], [4, 4, 4]).is_ok());
    assert!(box_cells([0; 3], [4, 4, 5]).is_err());
    assert!(box_cells([0; 3], [0, 1, 1]).is_err());
    assert!(box_cells([i32::MAX, 0, 0], [2, 1, 1]).is_err());
    assert_eq!(
        box_cells([-1, 0, 0], [2, 1, 2]).unwrap(),
        [[-1, 0, 0], [-1, 0, 1], [0, 0, 0], [0, 0, 1]]
    );
}
