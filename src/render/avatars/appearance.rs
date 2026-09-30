//! Compile only startup-verified palette data into the existing avatar shader.
//! Three fixed 32-color tables bound GPU state, independent of player count.
use crate::content::Catalog;
use std::fmt::Write;

pub(super) fn shader(catalog: &Catalog) -> String {
    let mut palettes = String::new();
    for (part, name) in ["SKINS", "SHIRTS", "PANTS"].iter().enumerate() {
        writeln!(palettes, "const {name} = array<vec3<f32>, 32>(").unwrap();
        for index in 0..32 {
            let color = catalog
                .appearance_color(part, index)
                .unwrap_or_else(|| catalog.appearance_color(part, 0).unwrap());
            writeln!(
                palettes,
                "vec3<f32>({:?}, {:?}, {:?}),",
                color[0], color[1], color[2]
            )
            .unwrap();
        }
        palettes.push_str(");\n");
    }
    include_str!("shader.wgsl").replace("// REGISTERED_PALETTES", &palettes)
}
