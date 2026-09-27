use super::*;

#[test]
fn workstation_summary_is_bounded_and_rejects_impossible_slots() {
    let view = WorkstationView {
        lit: true,
        fuel: 40,
        progress: 127,
        slots: [Some(Stack::new(crate::items::STICK, 128)), None, None],
        ..Default::default()
    };
    let bytes = view.encode();
    assert_eq!(WorkstationView::decode(&bytes), Some(view));
    for length in 0..24 {
        assert!(WorkstationView::decode(&bytes[..length]).is_none());
    }
    let mut invalid = bytes.clone();
    invalid[10..12].copy_from_slice(&129u16.to_le_bytes());
    assert!(WorkstationView::decode(&invalid).is_none());
    invalid = bytes;
    invalid[2] = 0;
    assert!(WorkstationView::decode(&invalid).is_none());
}
