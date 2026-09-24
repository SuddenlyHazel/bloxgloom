use std::io::Cursor;

use crate::items::{SAPLING, SEEDS, STICK};
use crate::world::{
    BLUE_FLOWER, DIRT, FERN, GLOWSTONE, GRASS, GRAVEL, LEAVES, MOSS, RED_FLOWER, SAND, SNOW, STONE,
    TALL_GRASS, WOOD, YELLOW_FLOWER,
};

pub(super) const TEXTURE_SIZE: u32 = 128;
pub(super) const TEXTURE_LAYERS: u32 = 20;
pub(super) const TEXTURE_MIPS: u32 = 8;
pub(super) const GLOWSTONE_LAYER: u8 = 8;

/// Keep directional block textures upright on both terrain quads and item cubes.
#[inline]
pub(super) fn face_uv(axis: usize, du: f32, dv: f32, width: f32, height: f32) -> (f32, f32) {
    match axis {
        0 => (dv, width - du),
        1 => (du, dv),
        _ => (du, height - dv),
    }
}

pub(super) fn material_layer(block: u8, axis: usize, side: i32) -> u8 {
    match block {
        GRASS if axis == 1 && side > 0 => 0,
        GRASS if axis == 1 => 2,
        GRASS => 1,
        DIRT => 2,
        STONE => 3,
        SAND => 4,
        SNOW => 5,
        MOSS => 6,
        GRAVEL => 7,
        GLOWSTONE => GLOWSTONE_LAYER,
        WOOD if axis == 1 => 10,
        WOOD => 9,
        LEAVES => 11,
        RED_FLOWER => 12,
        YELLOW_FLOWER => 13,
        BLUE_FLOWER => 14,
        FERN => 15,
        TALL_GRASS => 16,
        SEEDS => 17,
        SAPLING => 18,
        STICK => 19,
        _ => 3,
    }
}

pub(super) fn material_tiles() -> Vec<u8> {
    const SOURCES: [&[u8]; 20] = [
        include_bytes!("../../assets/textures/blocks/grass_top.png"),
        include_bytes!("../../assets/textures/blocks/grass_side.png"),
        include_bytes!("../../assets/textures/blocks/dirt.png"),
        include_bytes!("../../assets/textures/blocks/stone.png"),
        include_bytes!("../../assets/textures/blocks/sand.png"),
        include_bytes!("../../assets/textures/blocks/snow.png"),
        include_bytes!("../../assets/textures/blocks/moss.png"),
        include_bytes!("../../assets/textures/blocks/gravel.png"),
        include_bytes!("../../assets/textures/blocks/glowstone.png"),
        include_bytes!("../../assets/textures/blocks/wood_side.png"),
        include_bytes!("../../assets/textures/blocks/wood_top.png"),
        include_bytes!("../../assets/textures/foliage/leaves.png"),
        include_bytes!("../../assets/textures/foliage/flower_red.png"),
        include_bytes!("../../assets/textures/foliage/flower_yellow.png"),
        include_bytes!("../../assets/textures/foliage/flower_blue.png"),
        include_bytes!("../../assets/textures/foliage/fern.png"),
        include_bytes!("../../assets/textures/foliage/tall_grass.png"),
        include_bytes!("../../assets/textures/items/seeds.png"),
        include_bytes!("../../assets/textures/items/sapling.png"),
        include_bytes!("../../assets/textures/items/stick.png"),
    ];
    let mut pixels =
        Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * TEXTURE_LAYERS * 4) as usize);
    for (layer, source) in SOURCES.into_iter().enumerate() {
        let layer_start = pixels.len();
        let mut decoder = png::Decoder::new(Cursor::new(source));
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
                pixels.push(if layer >= 11 && channels == 4 {
                    decoded[index + 3]
                } else {
                    255
                });
            }
        }
        if layer < 11 {
            stitch_material_edges(&mut pixels[layer_start..], layer != 1);
        }
    }
    pixels
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

pub(super) fn material_mips() -> Vec<Vec<u8>> {
    let mut levels = Vec::with_capacity(TEXTURE_MIPS as usize);
    levels.push(material_tiles());
    for level in 1..TEXTURE_MIPS {
        let previous_size = TEXTURE_SIZE >> (level - 1);
        let size = TEXTURE_SIZE >> level;
        let previous = levels.last().unwrap();
        let previous_layer_bytes = (previous_size * previous_size * 4) as usize;
        let mut pixels = Vec::with_capacity((size * size * TEXTURE_LAYERS * 4) as usize);
        for layer in 0..TEXTURE_LAYERS as usize {
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
                        pixels.push(if alpha == 0 {
                            0
                        } else {
                            (weighted / alpha) as u8
                        });
                    }
                    pixels.push((alpha / 4) as u8);
                }
            }
        }
        levels.push(pixels);
    }
    levels
}
