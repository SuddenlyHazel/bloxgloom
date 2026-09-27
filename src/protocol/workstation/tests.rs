use super::*;

#[test]
fn registered_inventory_view_checks_exact_shape_permissions_metadata_and_status_bounds() {
    let catalog = crate::content::Catalog::builtins();
    let screen = catalog
        .inventory_screen(crate::content::KILN_ENTITY_TYPE)
        .unwrap();
    let view = WorkstationView {
        slots: vec![Some(Stack::new(crate::items::STICK, 128)), None, None],
        status: vec![16000, 500],
    };
    let bytes = view.encode();
    assert_eq!(WorkstationView::decode(&bytes), Some(view.clone()));
    assert!(view.valid_for(screen, &catalog));
    for length in 0..bytes.len() {
        assert!(WorkstationView::decode(&bytes[..length]).is_none());
    }
    let mut wrong = view.clone();
    wrong.status[0] = 96001;
    assert!(!wrong.valid_for(screen, &catalog));
    wrong.status = vec![];
    assert!(!wrong.valid_for(screen, &catalog));
    let mut invalid = bytes;
    invalid[7..9].copy_from_slice(&129u16.to_le_bytes());
    assert!(WorkstationView::decode(&invalid).is_none());
}

#[test]
fn inventory_view_is_bounded_and_never_exports_components() {
    let stack = Stack::with_components(crate::items::STICK, 128, 1, vec![99]).unwrap();
    let view = WorkstationView {
        slots: vec![Some(stack); MAX_SLOTS],
        status: vec![],
    };
    let bytes = view.encode();
    let decoded = WorkstationView::decode(&bytes).unwrap();
    assert_eq!(decoded.slots.len(), MAX_SLOTS);
    assert!(
        decoded
            .slots
            .iter()
            .flatten()
            .all(|s| s.components.is_none())
    );
    let mut invalid = bytes;
    invalid[1] = MAX_SLOTS as u8 + 1;
    assert!(WorkstationView::decode(&invalid).is_none());
}
