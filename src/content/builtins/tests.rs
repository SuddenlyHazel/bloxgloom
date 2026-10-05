use super::*;

#[test]
fn cached_builtins_keep_startup_registration_and_connection_caches_independent() {
    let mut first = Catalog::builtins();
    let second = Catalog::builtins();
    assert!(!std::sync::Arc::ptr_eq(
        &first.item_visuals,
        &second.item_visuals
    ));
    let fingerprint = second.fingerprint();
    let mut extra = first.block(world::STONE).unwrap().clone();
    extra.id = BlockTypeId(70_000);
    extra.key = "test:independent_builtin_extension".into();
    first.register_block(extra).unwrap();
    first
        .register_state(BlockStateId(70_000), BlockTypeId(70_000), vec![], None)
        .unwrap();
    assert!(first.block_type(BlockTypeId(70_000)).is_some());
    assert!(second.block_type(BlockTypeId(70_000)).is_none());
    assert!(second.state(BlockStateId(70_000)).is_none());
    let third = Catalog::builtins();
    assert!(third.block_type(BlockTypeId(70_000)).is_none());
    assert_eq!(third.fingerprint(), fingerprint);
    assert!(!std::sync::Arc::ptr_eq(
        &first.item_visuals,
        &third.item_visuals
    ));
    // Texture storage stays borrowed rather than copying the entire imported pack.
    let texture = second.texture(TextureId(0)).unwrap();
    assert!(matches!(texture.png, Cow::Borrowed(_)));
    assert_eq!(
        texture.png.as_ptr(),
        third.texture(TextureId(0)).unwrap().png.as_ptr()
    );
}
