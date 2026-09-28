use super::*;

fn storage() -> StorageBlockEntity {
    StorageBlockEntity {
        entity: "test:store".into(),
        block: "test:store".into(),
        placement_item: "test:store".into(),
        anchor_state: "test:store".into(),
        footprint: vec![FootprintCell {
            offset: [0; 3],
            state: "test:store".into(),
        }],
        slots: 2,
        automation_faces: None,
    }
}

#[test]
fn storage_faces_default_to_all_cardinal_normals_and_restrict_explicitly() {
    let mut declaration = storage();
    declaration.validate().unwrap();
    assert_eq!(
        declaration.allowed_automation_faces(),
        &crate::machine::FACES
    );
    declaration.automation_faces = Some(vec![[0, 1, 0]]);
    declaration.validate().unwrap();
    assert_eq!(declaration.allowed_automation_faces(), &[[0, 1, 0]]);
}

#[test]
fn storage_faces_reject_empty_duplicate_and_non_cardinal_lists() {
    let mut declaration = storage();
    for faces in [
        vec![],
        vec![[0, 1, 0], [0, 1, 0]],
        vec![[1, 1, 0]],
        vec![[0, 0, 0]],
    ] {
        declaration.automation_faces = Some(faces);
        assert!(declaration.validate().is_err());
    }
}
