//! Filter directions and carry unresolved normal variation into GGX roughness.
use super::{TEXTURE_MIPS, TEXTURE_SIZE};

pub(super) fn prepare(
    normal: Vec<u8>,
    specular: Vec<u8>,
    lab_layers: &[bool],
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let stride = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    let layers = normal.len() / stride;
    let mut normals = vec![normal];
    let mut materials = vec![specular];
    for level in 1..TEXTURE_MIPS {
        let size = (TEXTURE_SIZE >> level) as usize;
        normals.push(Vec::with_capacity(layers * size * size * 4));
        materials.push(Vec::with_capacity(layers * size * size * 4));
    }
    // Keep moments for one layer at a time, rather than an additional full GPU
    // array's worth of CPU floats. Raw means retain variance across all levels;
    // renormalizing those means would lose it at the next downsample.
    for layer in 0..layers {
        let lab = lab_layers.get(layer).copied().unwrap_or(false);
        let mut moments: Vec<[f32; 4]> = normals[0][layer * stride..(layer + 1) * stride]
            .chunks_exact(4)
            .zip(materials[0][layer * stride..(layer + 1) * stride].chunks_exact(4))
            .map(|(n, s)| {
                let x = f32::from(n[0]) / 127.5 - 1.0;
                let y = f32::from(n[1]) / 127.5 - 1.0;
                let z = if lab {
                    (1.0 - x * x - y * y).max(0.0).sqrt()
                } else {
                    f32::from(n[2]) / 127.5 - 1.0
                };
                let length = (x * x + y * y + z * z).sqrt().max(1e-8);
                let roughness = 1.0 - f32::from(s[0]) / 255.0;
                [x / length, y / length, z / length, roughness * roughness]
            })
            .collect();
        for level in 1..TEXTURE_MIPS as usize {
            let previous_size = (TEXTURE_SIZE >> (level - 1)) as usize;
            let size = previous_size / 2;
            let mut next = Vec::with_capacity(size * size);
            for y in 0..size {
                for x in 0..size {
                    let indices = [
                        y * 2 * previous_size + x * 2,
                        y * 2 * previous_size + x * 2 + 1,
                        (y * 2 + 1) * previous_size + x * 2,
                        (y * 2 + 1) * previous_size + x * 2 + 1,
                    ];
                    let mean: [f32; 4] = std::array::from_fn(|c| {
                        indices.iter().map(|&i| moments[i][c]).sum::<f32>() * 0.25
                    });
                    let length = (mean[0] * mean[0] + mean[1] * mean[1] + mean[2] * mean[2])
                        .sqrt()
                        .clamp(1e-8, 1.0);
                    let offset = layer * previous_size * previous_size * 4;
                    let averaged = |pixels: &[u8], channel: usize| -> u8 {
                        (indices
                            .iter()
                            .map(|&i| u32::from(pixels[offset + i * 4 + channel]))
                            .sum::<u32>()
                            / 4) as u8
                    };
                    let encode = |value: f32| ((value * 0.5 + 0.5) * 255.0).round() as u8;
                    let z = if lab {
                        averaged(&normals[level - 1], 2)
                    } else {
                        encode(mean[2] / length)
                    };
                    let height = averaged(&normals[level - 1], 3);
                    normals[level].extend([
                        encode(mean[0] / length),
                        encode(mean[1] / length),
                        z,
                        height,
                    ]);
                    // Toksvig-style unresolved direction variance broadens the
                    // lobe instead of vanishing when the mean is normalized.
                    let roughness = (mean[3] + (1.0 - length) / length).clamp(0.0, 1.0).sqrt();
                    let categorical =
                        |channel| materials[level - 1][offset + indices[0] * 4 + channel];
                    let green = if lab {
                        categorical(1)
                    } else {
                        averaged(&materials[level - 1], 1)
                    };
                    let blue = if lab {
                        categorical(2)
                    } else {
                        averaged(&materials[level - 1], 2)
                    };
                    let emission = averaged(&materials[level - 1], 3);
                    materials[level].extend([
                        ((1.0 - roughness) * 255.0).round() as u8,
                        green,
                        blue,
                        emission,
                    ]);
                    next.push(mean);
                }
            }
            moments = next;
        }
    }
    (normals, materials)
}
