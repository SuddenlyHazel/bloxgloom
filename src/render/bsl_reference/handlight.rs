//! Source hand light input mapping. No item state or lighting propagation changes.
/// Reference SH slots are otherwise unused; enhanced SH coefficients are untouched.
pub(crate) fn configure(camera: &mut [f32; 80], held: u8, relative_eye: [f32; 3]) {
    configure_for(camera, held, relative_eye, super::enabled());
}

fn configure_for(camera: &mut [f32; 80], held: u8, relative_eye: [f32; 3], reference: bool) {
    if !reference {
        return;
    }
    camera[76..79].copy_from_slice(&relative_eye.map(|v| if v.is_finite() { v } else { 0.0 }));
    camera[79] = f32::from(held.min(15));
}
#[cfg(test)]
mod tests;
