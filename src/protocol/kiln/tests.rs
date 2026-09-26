use super::*;

#[test]
fn workstation_summary_is_bounded_and_rejects_impossible_slots() {
    let view = KilnView {
        lit: true,
        fuel: 40,
        progress: 127,
        slots: [Some(Stack::new(crate::items::STICK, 128)), None, None],
        ..Default::default()
    };
    let bytes = view.encode();
    assert_eq!(KilnView::decode(&bytes), Some(view));
    for length in 0..24 {
        assert!(KilnView::decode(&bytes[..length]).is_none());
    }
    let mut invalid = bytes.clone();
    invalid[10..12].copy_from_slice(&129u16.to_le_bytes());
    assert!(KilnView::decode(&invalid).is_none());
    invalid = bytes;
    invalid[2] = 0;
    assert!(KilnView::decode(&invalid).is_none());
}
