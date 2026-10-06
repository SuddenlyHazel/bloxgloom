use super::*;

#[test]
fn appended_bark_wood_preserves_old_logs_and_supports_leaf_ecology() {
    let catalog = Catalog::builtins();
    let species = [
        "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
    ];
    for (offset, name) in species.into_iter().enumerate() {
        let log = catalog
            .state_by_key(&format!("bloxgloom:{name}_log[axis=y]"))
            .unwrap();
        let original_id = [15, 19, 23, 27, 31, 35, 39, 43, 47][offset];
        assert_eq!(log, BlockStateId(STATE_BASE + STATE_STRIDE * original_id));
        let source = catalog.state(log).unwrap();
        assert_ne!(
            source.textures.side, source.textures.top,
            "placed log retains cut ends"
        );
        for (axis, index) in [("x", 0), ("y", 1), ("z", 2)] {
            let timber = catalog.state_with_property(log, "axis", axis).unwrap();
            let state = catalog.state(timber).unwrap();
            assert_eq!(state.face_texture(index, 1), Some(source.textures.top));
            assert_eq!(state.face_texture(index, -1), Some(source.textures.bottom));
            let wood = catalog
                .state_by_key(&format!("bloxgloom:{name}_wood[axis={axis}]"))
                .unwrap();
            assert!(
                is_log(&catalog, wood),
                "living bough must sustain leaf ecology"
            );
            assert_eq!(
                catalog.state(wood).unwrap().face_textures,
                [source.textures.side; 6]
            );
        }
        let wood = catalog
            .state_by_key(&format!("bloxgloom:{name}_wood[axis=y]"))
            .unwrap();
        assert_eq!(
            wood,
            BlockStateId(STATE_BASE + STATE_STRIDE * (266 + offset as u32))
        );
        let item = catalog.primary_block_item(wood).unwrap();
        assert_eq!(catalog.item(item).unwrap().placeable, Some(wood));
        catalog.item_icon(item).unwrap().validate().unwrap();
    }
    assert_eq!(
        catalog.state(world::WOOD).unwrap().key,
        "bloxgloom:wood[axis=y]"
    );
}
