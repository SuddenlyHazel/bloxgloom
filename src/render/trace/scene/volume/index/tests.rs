use super::*;
#[test]
fn candidate_order_overlap_and_budget_preserve_legacy_admission() {
    let mut data = vec![0u32; 24];
    data[..8].copy_from_slice(&[(-64i32) as u32, (-64i32) as u32, 2, 0, 64, 1024, 0, 0]);
    data[8..16].copy_from_slice(&[(-32i32) as u32, (-32i32) as u32, 1, 0, 32, 1024, 0, 0]);
    data[16..24].copy_from_slice(&[0, 0, 1, 0, 32, 1024, 0, 0]);
    let original = data.clone();
    append(&mut data, 0, 3, 96);
    assert_eq!(
        data, original,
        "optional index must not break exact scene budget"
    );
    append(&mut data, 0, 3, u64::MAX);
    assert_eq!(data[7], MAGIC);
    let header = data[6] as usize;
    let lookup = |x: i32, z: i32| {
        let cell = (x - data[header] as i32) as usize
            + data[header + 2] as usize * (z - data[header + 1] as i32) as usize;
        let record = data[header + 4] as usize + cell * 2;
        let at = data[record] as usize;
        data[at..at + data[record + 1] as usize].to_vec()
    };
    assert_eq!(lookup(-1, -1), [0, 1]);
    assert_eq!(lookup(-4, -4), [0]);
    assert_eq!(lookup(0, 0), [2]);
    assert!(lookup(-4, 0).is_empty());
    let mut distant = original.clone();
    distant[16] = 1_000_000;
    let copy = distant.clone();
    append(&mut distant, 0, 3, u64::MAX);
    assert_eq!(distant, copy, "oversized index retains ordered scan");
}
