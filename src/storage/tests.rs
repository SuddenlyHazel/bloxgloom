use super::*;
use crate::content::{BlockDef, BlockTextures, Catalog, ItemDef, TextureDef};
use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

fn with_extra_block(key: &str) -> Catalog {
    let mut catalog = Catalog::builtins();
    let texture = catalog
        .register_texture(TextureDef {
            key: "example:marble_tile".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
        })
        .unwrap();
    catalog
        .register_block(BlockDef {
            id: 16,
            key: key.to_owned().into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            textures: BlockTextures {
                top: texture,
                side: texture,
                bottom: texture,
            },
            solid: true,
            opaque: true,
            cutout: false,
            plant: false,
            replaceable: false,
            supports_plant: false,
            emission: 0,
            reflectance: [180, 180, 180],
        })
        .unwrap();
    catalog
        .register_item(ItemDef {
            id: 131,
            key: key.to_owned().into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            texture,
            placeable: Some(16),
            sprite: false,
        })
        .unwrap();
    catalog
}

#[test]
fn content_map_allows_additions_but_rejects_reassigned_ids_and_corruption() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-content-map-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&path).unwrap();
    let base = Catalog::builtins();
    verify_content_map_with(&path, true, &base).unwrap();
    let extended = with_extra_block("example:marble");
    verify_content_map_with(&path, false, &extended).unwrap();
    assert_eq!(
        decode_content_map(&fs::read(path.join(CONTENT_MAP)).unwrap())
            .unwrap()
            .get(&(b'B', 16))
            .map(String::as_str),
        Some("example:marble")
    );
    assert_eq!(
        decode_content_map(&fs::read(path.join(CONTENT_MAP)).unwrap())
            .unwrap()
            .get(&(b'I', 131))
            .map(String::as_str),
        Some("example:marble")
    );
    assert!(verify_content_map_with(&path, false, &base).is_err());
    assert!(verify_content_map_with(&path, false, &with_extra_block("other:marble")).is_err());
    let mut corrupt = fs::read(path.join(CONTENT_MAP)).unwrap();
    corrupt[10] ^= 1;
    fs::write(path.join(CONTENT_MAP), corrupt).unwrap();
    assert!(verify_content_map_with(&path, false, &extended).is_err());
    fs::remove_dir_all(path).unwrap();
}
