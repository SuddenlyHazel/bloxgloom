use super::SUN;
use sha2::{Digest, Sha256};
use std::io::Cursor;

#[path = "gpu.rs"]
mod gpu;

// Principal-core metadata deliberately measures the brightest contiguous core,
// excluding the lower-intensity rings and secondary lens-flare ghosts.
fn measured_core() -> [f64; 2] {
    assert_eq!(
        format!("{:x}", Sha256::digest(SUN)),
        "976a853bc48bc37b294b671c1e32cca70218f05bf81c011d6a47b31b1ea2718e",
        "Sun artwork changed: remeasure the projection anchor"
    );
    let mut reader = png::Decoder::new(Cursor::new(SUN)).read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let image = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((image.width, image.height), (512, 512));
    assert_eq!(image.color_type, png::ColorType::Rgb);
    assert_eq!(image.bit_depth, png::BitDepth::Eight);
    let luminance: Vec<f64> = bytes[..image.buffer_size()]
        .chunks_exact(3)
        .map(|rgb| {
            rgb.iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(channel, weight)| {
                    let value = f64::from(*channel) / 255.0;
                    let linear = if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    };
                    linear * weight
                })
                .sum()
        })
        .collect();
    let peak = luminance.iter().copied().fold(0.0, f64::max);
    let mut weighted = [0.0; 2];
    let mut weight = 0.0;
    let mut count = 0;
    for (index, value) in luminance.into_iter().enumerate() {
        if value >= peak * 0.75 {
            weighted[0] += value * (f64::from(index as u32 % image.width) + 0.5);
            weighted[1] += value * (f64::from(index as u32 / image.width) + 0.5);
            weight += value;
            count += 1;
        }
    }
    assert_eq!(count, 579);
    weighted.map(|component| component / weight)
}

#[test]
fn unchanged_jg_sun_core_metadata_matches_actual_linear_srgb_artwork() {
    let actual = measured_core();
    for (actual, expected) in actual.into_iter().zip([227.6215, 216.7756]) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }
}
