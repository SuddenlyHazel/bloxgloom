use std::io::Cursor;

pub(super) mod resources;

use crate::content;
use crate::items::ItemId;
use crate::world::BlockId;

pub(super) const TEXTURE_SIZE: u32 = 128;
pub(super) const TEXTURE_MIPS: u32 = 8;
#[cfg(test)]
pub(super) fn texture_layers() -> u32 {
    content::catalog().textures().len() as u32
}

pub(super) fn texture_layers_for(catalog: &content::Catalog) -> u32 {
    catalog.textures().len() as u32
}

/// Keep directional block textures upright on both terrain quads and item cubes.
#[inline]
pub(super) fn face_uv(axis: usize, du: f32, dv: f32, width: f32, height: f32) -> (f32, f32) {
    match axis {
        0 => (dv, width - du),
        1 => (du, dv),
        _ => (du, height - dv),
    }
}

#[cfg(test)]
pub(super) fn material_layer(block_id: BlockId, axis: usize, side: i32) -> u32 {
    material_layer_for(content::catalog(), block_id, axis, side)
}

pub(super) fn material_layer_for(
    catalog: &content::Catalog,
    block_id: BlockId,
    axis: usize,
    side: i32,
) -> u32 {
    catalog
        .state(block_id)
        .and_then(|state| state.face_texture(axis, side))
        .map_or(3, |texture| texture.get())
}

#[cfg(test)]
pub(super) fn item_material_layer(item_id: ItemId, axis: usize, side: i32) -> u32 {
    item_material_layer_for(content::catalog(), item_id, axis, side)
}

pub(super) fn item_material_layer_for(
    catalog: &content::Catalog,
    item_id: ItemId,
    axis: usize,
    side: i32,
) -> u32 {
    let Some(item) = catalog.item(item_id) else {
        return 3;
    };
    if let Some(block) = item.placeable
        && !item.sprite
    {
        material_layer_for(catalog, block, axis, side)
    } else {
        item.texture.get()
    }
}

#[cfg(test)]
pub(super) fn material_tiles() -> Vec<u8> {
    material_tiles_for(content::catalog())
}

pub(super) fn material_tiles_for(catalog: &content::Catalog) -> Vec<u8> {
    let usage = resources::validate(catalog.textures().len(), resources::MAX_ARRAY_LAYERS)
        .expect("material resources must pass admission before pixel preparation");
    let mut pixels = Vec::with_capacity(
        (u64::from(TEXTURE_SIZE) * u64::from(TEXTURE_SIZE) * u64::from(usage.layers) * 4) as usize,
    );
    for definition in catalog.textures() {
        let layer_start = pixels.len();
        let mut decoder = png::Decoder::new(Cursor::new(definition.png.as_ref()));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("embedded material PNG is valid");
        let mut decoded = vec![0; reader.output_buffer_size().expect("material PNG size fits")];
        let info = reader
            .next_frame(&mut decoded)
            .expect("embedded material PNG decodes");
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            other => panic!("embedded material PNG must be RGB or RGBA, got {other:?}"),
        };
        for y in 0..TEXTURE_SIZE {
            let source_y = ((y * 2 + 1) * info.height / (2 * TEXTURE_SIZE)) as usize;
            for x in 0..TEXTURE_SIZE {
                let source_x = ((x * 2 + 1) * info.width / (2 * TEXTURE_SIZE)) as usize;
                let index = (source_y * info.width as usize + source_x) * channels;
                pixels.extend_from_slice(&decoded[index..index + 3]);
                pixels.push(if definition.alpha_cutout && channels == 4 {
                    decoded[index + 3]
                } else {
                    255
                });
            }
        }
        if definition.stitch_edges {
            stitch_material_edges(&mut pixels[layer_start..], definition.stitch_vertical);
        }
    }
    pixels
}

pub(super) fn emission_strengths(catalog: &content::Catalog) -> Vec<f32> {
    catalog
        .textures()
        .iter()
        .map(|texture| texture.emission_strength)
        .collect()
}

fn stitch_material_edges(pixels: &mut [u8], stitch_vertical: bool) {
    let size = TEXTURE_SIZE as usize;
    const BAND: usize = 4;
    for y in 0..size {
        for offset in 0..BAND {
            let left = (y * size + offset) * 4;
            let right = (y * size + size - 1 - offset) * 4;
            blend_opposite_pixels(pixels, left, right, BAND - offset, BAND);
        }
    }
    if stitch_vertical {
        for x in 0..size {
            for offset in 0..BAND {
                let top = (offset * size + x) * 4;
                let bottom = ((size - 1 - offset) * size + x) * 4;
                blend_opposite_pixels(pixels, top, bottom, BAND - offset, BAND);
            }
        }
    }
}

fn blend_opposite_pixels(pixels: &mut [u8], a: usize, b: usize, weight: usize, total: usize) {
    for channel in 0..3 {
        let first = usize::from(pixels[a + channel]);
        let second = usize::from(pixels[b + channel]);
        let shared = (first + second) / 2;
        pixels[a + channel] = ((first * (total - weight) + shared * weight) / total) as u8;
        pixels[b + channel] = ((second * (total - weight) + shared * weight) / total) as u8;
    }
}

#[cfg(test)]
pub(super) fn material_mips() -> Vec<Vec<u8>> {
    material_mips_for(content::catalog())
}

pub(super) fn material_mips_for(catalog: &content::Catalog) -> Vec<Vec<u8>> {
    let mut levels = Vec::with_capacity(TEXTURE_MIPS as usize);
    levels.push(material_tiles_for(catalog));
    for level in 1..TEXTURE_MIPS {
        let previous_size = TEXTURE_SIZE >> (level - 1);
        let size = TEXTURE_SIZE >> level;
        let previous = levels.last().unwrap();
        let previous_layer_bytes = (previous_size * previous_size * 4) as usize;
        let mut pixels =
            Vec::with_capacity((size * size * texture_layers_for(catalog) * 4) as usize);
        for layer in 0..texture_layers_for(catalog) as usize {
            for y in 0..size {
                for x in 0..size {
                    let offsets = [(0, 0), (1, 0), (0, 1), (1, 1)];
                    let samples = offsets.map(|(dx, dy)| {
                        let index = layer * previous_layer_bytes
                            + ((((y * 2 + dy) * previous_size + x * 2 + dx) * 4) as usize);
                        &previous[index..index + 4]
                    });
                    let alpha = samples.iter().map(|pixel| u32::from(pixel[3])).sum::<u32>();
                    for channel in 0..3 {
                        let weighted = samples
                            .iter()
                            .map(|pixel| u32::from(pixel[channel]) * u32::from(pixel[3]))
                            .sum::<u32>();
                        pixels.push(weighted.checked_div(alpha).unwrap_or(0) as u8);
                    }
                    pixels.push((alpha / 4) as u8);
                }
            }
        }
        levels.push(pixels);
    }
    levels
}
