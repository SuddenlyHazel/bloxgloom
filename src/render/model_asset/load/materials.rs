//! Embedded PNG decoding and base-color/cutout material preservation.
use super::*;
use gltf::material::AlphaMode;

pub(super) fn load(g: &gltf::Gltf, blob: &[u8]) -> Result<(Vec<Material>, Vec<Image>)> {
    ensure(
        g.materials().len() > 0 && g.materials().len() <= 32 && g.images().len() <= 32,
        "model material/image limits exceeded",
    )?;
    let mut images = Vec::new();
    let mut total_bytes = 0;
    for image in g.images() {
        let gltf::image::Source::View { view, mime_type } = image.source() else {
            return Err(
                "embed textures inside the GLB; external image paths are unsupported".into(),
            );
        };
        ensure(
            mime_type == "image/png",
            "model textures must be embedded PNG images",
        )?;
        let data = &blob[view.offset()..view.offset() + view.length()];
        let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        decoder.set_limits(png::Limits {
            bytes: 16 * 1024 * 1024,
        });
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("invalid model PNG: {e}"))?;
        let info = reader.info();
        ensure(
            info.width > 0 && info.height > 0 && info.width <= 2048 && info.height <= 2048,
            "model texture dimensions must be 1..2048",
        )?;
        let (width, height) = (info.width, info.height);
        total_bytes += width as usize * height as usize * 4;
        ensure(
            total_bytes <= MAX_BYTES,
            "model decoded textures exceed 64 MiB",
        )?;
        let mut pixels = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or("model PNG output is too large")?
        ];
        let info = reader
            .next_frame(&mut pixels)
            .map_err(|e| format!("invalid model PNG pixels: {e}"))?;
        let pixels = &pixels[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => pixels.to_vec(),
            png::ColorType::Rgb => pixels
                .chunks_exact(3)
                .flat_map(|v| [v[0], v[1], v[2], 255])
                .collect(),
            png::ColorType::Grayscale => pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
            png::ColorType::GrayscaleAlpha => pixels
                .chunks_exact(2)
                .flat_map(|v| [v[0], v[0], v[0], v[1]])
                .collect(),
            _ => return Err("unsupported model PNG color type".into()),
        };
        images.push(Image {
            width,
            height,
            rgba,
        });
    }
    let mut materials = Vec::new();
    for material in g.materials() {
        ensure(
            material.alpha_mode() != AlphaMode::Blend,
            "transparent BLEND materials are not supported yet; use alpha cutout",
        )?;
        let pbr = material.pbr_metallic_roughness();
        ensure(
            material.normal_texture().is_none()
                && material.occlusion_texture().is_none()
                && material.emissive_texture().is_none()
                && material.emissive_factor() == [0.0; 3]
                && pbr.metallic_roughness_texture().is_none(),
            "model companion/emissive maps are not supported yet",
        )?;
        let color = pbr.base_color_factor();
        ensure(
            color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid material base color",
        )?;
        let texture = pbr.base_color_texture();
        if let Some(texture) = &texture {
            ensure(
                texture.tex_coord() == 0,
                "model materials must use UV set zero",
            )?;
        }
        let wrap = texture
            .as_ref()
            .map_or([gltf::texture::WrappingMode::ClampToEdge; 2], |t| {
                [
                    t.texture().sampler().wrap_s(),
                    t.texture().sampler().wrap_t(),
                ]
            });
        let alpha_cutoff = (material.alpha_mode() == AlphaMode::Mask)
            .then(|| material.alpha_cutoff().unwrap_or(0.5));
        ensure(
            alpha_cutoff.is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
            "invalid material alpha cutoff",
        )?;
        let source_texture = texture.as_ref().map(|t| t.texture());
        materials.push(Material {
            name: named(
                material
                    .name()
                    .or_else(|| source_texture.as_ref().and_then(|t| t.name())),
                "material",
                material.index().unwrap(),
            )?,
            texture: texture.map(|t| t.texture().source().index()),
            color,
            alpha_cutoff,
            double_sided: material.double_sided(),
            wrap,
        });
    }
    Ok((materials, images))
}
