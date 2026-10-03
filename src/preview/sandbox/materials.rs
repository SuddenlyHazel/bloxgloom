//! Minimal preview-only industrial surfaces. No gameplay identities or asset dependencies.
use super::*;

/// Register ordinary materials before the preview catalog is frozen. These never enter
/// normal world catalogs, and use the same public texture/block path as creator content.
pub fn install_sandbox_materials(catalog: &mut Catalog) -> Result<(), Box<dyn Error>> {
    use bloxgloom_host_api::content as api;
    for (name, color, emissive) in [
        ("cyan", [30u8, 218, 241], true),
        ("magenta", [233, 52, 164], true),
        ("steel", [186, 190, 194], false),
    ] {
        let key = format!("sandbox:{name}");
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 16, 16);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut pixels = Vec::with_capacity(16 * 16 * 4);
            for y in 0..16 {
                for x in 0..16 {
                    let edge = x == 0 || y == 0 || x == 15 || y == 15;
                    let rivet = [2, 13].contains(&x) && [2, 13].contains(&y);
                    let scale = if edge {
                        0.42
                    } else if !emissive && rivet {
                        1.08
                    } else if !emissive {
                        0.93 + ((x * 17 + y * 7) % 11) as f32 * 0.014
                    } else {
                        1.0
                    };
                    pixels.extend(color.map(|v| (f32::from(v) * scale) as u8));
                    pixels.push(255);
                }
            }
            encoder.write_header()?.write_image_data(&pixels)?;
        }
        catalog.public_texture(&api::Texture {
            key: key.clone(),
            png: std::borrow::Cow::Owned(png),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: if emissive { 3.5 } else { 0.0 },
            foliage: Default::default(),
        })?;
        catalog.public_block(&api::Block {
            key: key.clone(),
            name: format!("SANDBOX {} PANEL", name.to_uppercase()),
            swatch: [
                f32::from(color[0]) / 255.0,
                f32::from(color[1]) / 255.0,
                f32::from(color[2]) / 255.0,
                1.0,
            ],
            textures: api::FaceTextures::uniform(key),
            geometry: api::Geometry::Cube,
            material: api::Material::Opaque,
            solid: true,
            replaceable: false,
            supports_plant: false,
            flammable: false,
            emission: if emissive { 15 } else { 0 },
            sky_attenuation: 0,
            reflectance: color,
            acoustics: None,
            properties: vec![],
            states: vec![api::BlockState::default()],
        })?;
    }
    install_steel_companions(catalog)?;
    // wgpu-hal 30 GLES allocates square D2 arrays with a multiple of six layers
    // as cube arrays before the explicit D2Array view is created. Keep this
    // software-preview workaround local; no block references the reserved tile.
    if catalog.textures().len().is_multiple_of(6) {
        let png = catalog
            .textures()
            .last()
            .expect("fixture textures")
            .png
            .clone();
        catalog.public_texture(&api::Texture {
            key: "sandbox:gles_reserved".into(),
            png,
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })?;
    }
    catalog
        .validate()
        .map_err(|error| format!("sandbox catalog: {error:?}"))?;
    Ok(())
}

/// Companion maps match the original riveted panel layout, using oldPBR linear
/// data (R smoothness, G metalness). No external texture or new gameplay content.
fn install_steel_companions(catalog: &mut Catalog) -> Result<(), Box<dyn Error>> {
    use bloxgloom_host_api::content as api;
    for suffix in ["n", "s"] {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 16, 16);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut pixels = Vec::with_capacity(16 * 16 * 4);
            for y in 0..16 {
                for x in 0..16 {
                    let edge = x == 0 || y == 0 || x == 15 || y == 15;
                    let rivet = [2, 13].contains(&x) && [2, 13].contains(&y);
                    let pixel = if suffix == "s" {
                        // Satin panel with rough seams and slightly smoother rivets.
                        [
                            if edge {
                                75
                            } else if rivet {
                                205
                            } else {
                                160
                            },
                            if edge { 0 } else { 255 },
                            0,
                            255,
                        ]
                    } else {
                        // Very shallow pressed seams; flat panel remains stable at distance.
                        [
                            if x == 1 {
                                116
                            } else if x == 14 {
                                140
                            } else {
                                128
                            },
                            if y == 1 {
                                116
                            } else if y == 14 {
                                140
                            } else {
                                128
                            },
                            254,
                            if edge { 210 } else { 255 },
                        ]
                    };
                    pixels.extend(pixel);
                }
            }
            encoder.write_header()?.write_image_data(&pixels)?;
        }
        catalog.public_texture(&api::Texture {
            key: format!("sandbox:steel_{suffix}"),
            png: std::borrow::Cow::Owned(png),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
