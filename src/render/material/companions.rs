//! Catalog-key companion lookup and linear-data mip preparation, off the window thread.
use super::{TEXTURE_MIPS, TEXTURE_SIZE};
use crate::content::{Catalog, TextureDef};
use std::{collections::HashMap, io::Cursor};

pub(crate) struct Maps {
    pub normal: Vec<Vec<u8>>,
    pub specular: Vec<Vec<u8>>,
    pub flags: Vec<u32>,
}

pub(crate) fn prepare(catalog: &Catalog) -> Maps {
    let definitions: HashMap<_, _> = catalog
        .textures()
        .iter()
        .map(|t| (t.key.as_ref(), t))
        .collect();
    let mut normal = Vec::new();
    let mut specular = Vec::new();
    let mut flags = Vec::new();
    for texture in catalog.textures() {
        let n = definitions
            .get(format!("{}_n", texture.key).as_str())
            .copied();
        let s = definitions
            .get(format!("{}_s", texture.key).as_str())
            .copied();
        // Quantized presentation parameters reuse the existing flags buffer.
        // Zero metadata leaves generic cutouts and industrial materials unchanged.
        flags.push(
            u32::from(n.is_some())
                | (u32::from(s.is_some()) << 1)
                | (((texture.foliage.wrap * 255.0).round() as u32) << 8)
                | (((texture.foliage.transmission * 255.0).round() as u32) << 16),
        );
        normal.extend(tile(n, [128, 128, 255, 255]));
        specular.extend(tile(s, [0, 0, 0, 255]));
    }
    Maps {
        normal: mips(normal),
        specular: mips(specular),
        flags,
    }
}

fn tile(definition: Option<&TextureDef>, fallback: [u8; 4]) -> Vec<u8> {
    let Some(definition) = definition else {
        return fallback.repeat((TEXTURE_SIZE * TEXTURE_SIZE) as usize);
    };
    let mut decoder = png::Decoder::new(Cursor::new(definition.png.as_ref()));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("verified companion PNG");
    let mut data = vec![0; reader.output_buffer_size().expect("bounded companion PNG")];
    let info = reader
        .next_frame(&mut data)
        .expect("verified companion PNG decodes");
    let channels = if info.color_type == png::ColorType::Rgba {
        4
    } else {
        3
    };
    let mut pixels = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let sx = ((x * 2 + 1) * info.width / (2 * TEXTURE_SIZE)) as usize;
            let sy = ((y * 2 + 1) * info.height / (2 * TEXTURE_SIZE)) as usize;
            let i = (sy * info.width as usize + sx) * channels;
            pixels.extend_from_slice(&data[i..i + 3]);
            pixels.push(if channels == 4 { data[i + 3] } else { 255 });
        }
    }
    pixels
}

fn mips(base: Vec<u8>) -> Vec<Vec<u8>> {
    let layers = base.len() / (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    let mut levels = vec![base];
    for level in 1..TEXTURE_MIPS {
        let previous_size = (TEXTURE_SIZE >> (level - 1)) as usize;
        let size = previous_size / 2;
        let previous = levels.last().unwrap();
        let mut pixels = Vec::with_capacity(layers * size * size * 4);
        for layer in 0..layers {
            for y in 0..size {
                for x in 0..size {
                    for channel in 0..4 {
                        let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                            .into_iter()
                            .map(|(dx, dy)| {
                                u32::from(
                                    previous[(layer * previous_size * previous_size
                                        + (y * 2 + dy) * previous_size
                                        + x * 2
                                        + dx)
                                        * 4
                                        + channel],
                                )
                            })
                            .sum();
                        pixels.push((sum / 4) as u8);
                    }
                }
            }
        }
        levels.push(pixels);
    }
    levels
}

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    levels: &[Vec<u8>],
    layers: u32,
    label: &str,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: layers,
        },
        mip_level_count: TEXTURE_MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in levels.iter().enumerate() {
        let size = TEXTURE_SIZE >> level;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests;
