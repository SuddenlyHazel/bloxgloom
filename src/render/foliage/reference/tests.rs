use super::*;

#[test]
fn frozen_builtin_wind_classes_preserve_source_stationary_and_tall_blocks() {
    let catalog = Catalog::builtins();
    let class = |key: &str| {
        wind_class(
            catalog
                .textures()
                .iter()
                .find(|t| t.key.as_ref() == key)
                .unwrap(),
        )
    };
    for (key, expected) in [
        ("bloxgloom:leaves", 105),
        ("bloxgloom:fern", 100),
        ("bloxgloom:flower_red", 101),
        ("bloxgloom:jg_cherry_leaves", 105),
        ("bloxgloom:jg_short_grass", 100),
        ("bloxgloom:jg_tall_grass_bottom", 102),
        ("bloxgloom:jg_tall_grass_top", 103),
        ("bloxgloom:jg_vine", 106),
        ("bloxgloom:jg_pink_petals", 107),
        ("bloxgloom:jg_dead_bush", 109),
        ("bloxgloom:jg_oak_sapling", 109),
        ("bloxgloom:jg_red_mushroom", 109),
    ] {
        assert_eq!(class(key), expected, "{key}");
    }
    let source = shader(&catalog);
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn checked_in_source_settings_and_distant_vertices_have_expected_defaults() {
    let settings = include_str!("../../../../external/shaders/lib/settings.glsl");
    for name in [
        "WAVING_GRASS",
        "WAVING_CROP",
        "WAVING_PLANT",
        "WAVING_TALL_PLANT",
        "WAVING_LEAF",
        "WAVING_VINE",
    ] {
        assert!(
            settings
                .lines()
                .any(|line| line.trim() == format!("#define {name}")),
            "{name}"
        );
    }
    assert!(
        settings
            .lines()
            .any(|line| line.trim() == "//#define WORLD_TIME_ANIMATION")
    );
    for name in ["ANIMATION_STRENGTH", "ANIMATION_SPEED"] {
        assert!(
            settings
                .lines()
                .any(|line| line.trim().starts_with(&format!("#define {name} 1.00 //"))),
            "{name}"
        );
    }
    let distant = include_str!("../../../../external/shaders/program/dh_terrain.glsl");
    assert!(
        !distant.contains("WavingBlocks("),
        "source DH terrain includes the helper but keeps coarse vegetation stationary"
    );
}

mod callers;
mod gpu;
mod oracle;
