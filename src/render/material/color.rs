//! Byte albedo filters operate in linear light; alpha remains linear coverage.
use std::sync::OnceLock;

struct Transfer {
    linear: [u32; 256],
    srgb: [u8; 65536],
}

fn transfer() -> &'static Transfer {
    static TABLES: OnceLock<Transfer> = OnceLock::new();
    TABLES.get_or_init(|| Transfer {
        linear: std::array::from_fn(|value| {
            let srgb = value as f64 / 255.0;
            let linear = if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            };
            (linear * 65535.0).round() as u32
        }),
        srgb: std::array::from_fn(|value| {
            let linear = value as f64 / 65535.0;
            let srgb = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            (srgb * 255.0).round() as u8
        }),
    })
}

pub(super) fn downsample(samples: [&[u8]; 4]) -> [u8; 4] {
    let tables = transfer();
    let alpha: u32 = samples.iter().map(|pixel| u32::from(pixel[3])).sum();
    let mut result = [0, 0, 0, (alpha / 4) as u8];
    if alpha == 0 {
        return result;
    }
    for channel in 0..3 {
        let weighted: u32 = samples
            .iter()
            .map(|pixel| tables.linear[usize::from(pixel[channel])] * u32::from(pixel[3]))
            .sum();
        result[channel] = tables.srgb[((weighted + alpha / 2) / alpha) as usize];
    }
    result
}

pub(super) fn blend(first: u8, second: u8, weight: usize, total: usize) -> (u8, u8) {
    let tables = transfer();
    let a = tables.linear[usize::from(first)] as usize;
    let b = tables.linear[usize::from(second)] as usize;
    let shared = (a + b) / 2;
    (
        tables.srgb[(a * (total - weight) + shared * weight) / total],
        tables.srgb[(b * (total - weight) + shared * weight) / total],
    )
}

#[cfg(test)]
mod tests;
