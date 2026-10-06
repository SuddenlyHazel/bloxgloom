//! Source-reference presentation inputs from authoritative inventory and completed lights.
use super::*;
fn selected_emission(
    inventory: &Inventory,
    selected: usize,
    catalog: &crate::content::Catalog,
) -> u8 {
    inventory
        .slots
        .get(selected)
        .and_then(Option::as_ref)
        .filter(|stack| stack.count > 0)
        .and_then(|stack| crate::items::placeable_block_in(stack.item, catalog))
        .map_or(0, |block| catalog.emission(block).min(15))
}
// Iris relativeEyePosition = cameraPosition - eyePosition. Source shaders add
// it to camera-relative world positions to recover player-eye-relative space.
fn relative_eye(view: Vec3, player_eye: Vec3) -> [f32; 3] {
    (view - player_eye).to_array()
}

impl ClientApp {
    pub(super) fn configure_reference_lighting(&mut self, camera: Camera) {
        if !crate::render::bsl_reference::enabled() {
            return;
        }
        let held = selected_emission(&self.inventory, self.config.selected_slot, &self.catalog);
        let relative_eye = relative_eye(camera.position, self.camera().position);
        let Some(renderer) = &self.renderer else {
            return;
        };
        let mesh = renderer.reference_rain_mesh(camera);
        let glow = mesh
            .chunks_exact(9)
            .map(|v| f32::from(self.light_at(Vec3::new(v[0], v[1], v[2])).glow) / 15.0)
            .collect::<Vec<_>>();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_reference_handlight(held, relative_eye);
            renderer.set_reference_rain_mesh(mesh, glow);
        }
    }
}
#[cfg(test)]
mod tests;
