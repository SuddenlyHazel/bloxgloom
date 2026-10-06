//! RGB9E5 shared-exponent encoding retains dark linear fluid tint in four bytes.
pub(super) fn encode(rgb: [f32; 3]) -> u32 {
    let rgb = rgb.map(|c| c.clamp(0.0, 1.0));
    let largest = rgb.into_iter().fold(0.0_f32, f32::max);
    if largest == 0.0 {
        return 0;
    }
    let mut exponent = (largest.log2().floor() as i32 + 16).clamp(0, 31);
    if (largest * 2.0_f32.powi(24 - exponent)).round() >= 512.0 {
        exponent += 1;
    }
    let scale = 2.0_f32.powi(24 - exponent);
    let mantissas = rgb.map(|c| (c * scale).round().min(511.0) as u32);
    mantissas[0] | (mantissas[1] << 9) | (mantissas[2] << 18) | ((exponent as u32) << 27)
}

pub(super) fn decode(bits: u32) -> [f32; 3] {
    let scale = 2.0_f32.powi((bits >> 27) as i32 - 24);
    [bits & 511, (bits >> 9) & 511, (bits >> 18) & 511].map(|v| v as f32 * scale)
}
