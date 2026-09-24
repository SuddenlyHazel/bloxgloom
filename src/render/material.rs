use std::io::Cursor;

use crate::world::{DIRT, GLOWSTONE, GRASS, GRAVEL, MOSS, SAND, SNOW, STONE};

pub(super) const TEXTURE_SIZE: u32 = 128;
pub(super) const TEXTURE_LAYERS: u32 = 9;
pub(super) const TEXTURE_MIPS: u32 = 8;

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
        GLOWSTONE => 8,
        _ => 3,
    }
}

pub(super) fn material_tiles() -> Vec<u8> {
    const SOURCES: [&[u8]; 9] = [
        include_bytes!("../../assets/textures/grass_top.png"),
        include_bytes!("../../assets/textures/grass_side.png"),
        include_bytes!("../../assets/textures/dirt.png"),
        include_bytes!("../../assets/textures/stone.png"),
        include_bytes!("../../assets/textures/sand.png"),
        include_bytes!("../../assets/textures/snow.png"),
        include_bytes!("../../assets/textures/moss.png"),
        include_bytes!("../../assets/textures/gravel.png"),
        include_bytes!("../../assets/textures/glowstone.png"),
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
                pixels.push(255);
            }
        }
        stitch_material_edges(&mut pixels[layer_start..], layer != 1);
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
                    for channel in 0..4usize {
                        let mut sum = 0u16;
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let index = layer * previous_layer_bytes
                                    + ((((y * 2 + dy) * previous_size + x * 2 + dx) * 4) as usize)
                                    + channel;
                                sum += u16::from(previous[index]);
                            }
                        }
                        pixels.push((sum / 4) as u8);
                    }
                }
            }
        }
        levels.push(pixels);
    }
    levels
}
