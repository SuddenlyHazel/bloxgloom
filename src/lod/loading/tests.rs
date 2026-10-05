use super::*;
#[test]
fn cpu_policy_bounds_both_pools_and_leaves_capacity_for_gameplay() {
    for cores in 0..=128 {
        let count = workers_for(cores);
        assert!((1..=4).contains(&count));
        if cores >= 4 {
            assert!(count * 2 <= cores - 2);
        }
    }
    assert_eq!(workers_for(1), 1);
    assert_eq!(workers_for(8), 3);
    assert_eq!(workers_for(32), 4);
}
