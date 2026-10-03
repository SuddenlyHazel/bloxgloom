//! Session configuration and rendered-frame admission, independent of drawing.
use crate::render::{Camera, Renderer, local_shadow};
impl Renderer {
    pub(crate) fn configure_local_shadows(&mut self, user: local_shadow::Settings) {
        if user == self.user_local_shadows
            && self.package_local_shadows == self.applied_package_local_shadows
        {
            return;
        }
        self.user_local_shadows = user;
        self.applied_package_local_shadows = self.package_local_shadows;
        let requested = self.package_local_shadows.unwrap_or(user);
        let settings = local_shadow::Settings {
            count: requested.count.min(user.count),
            resolution: requested.resolution.min(user.resolution),
            range: requested.range.min(user.range),
            updates: requested.updates.min(user.updates),
        }
        .with_environment(self.device.limits().max_texture_dimension_2d);
        if self.local_shadows.settings == settings {
            return;
        }
        self.local_shadows = local_shadow::LocalShadows::new_with_settings(
            &self.device,
            &self.camera_buffer,
            settings,
        );
        self.sun_shadows
            .bind_local(&self.device, &self.camera_buffer, &self.local_shadows);
        self.camera_group = self.sun_shadows.camera_group.clone();
        self.avatars.set_camera_group(self.camera_group.clone());
    }

    pub(in crate::render) fn prepare_local_shadows(&mut self, camera: Camera) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.local_shadow_frame).as_secs_f32();
        self.local_shadow_frame = now;
        if self.local_shadows.settings.count == 0 {
            return;
        }
        let sources: Vec<_> = self
            .meshes
            .values()
            .flat_map(|mesh| mesh.local_sources.iter().copied())
            .collect();
        self.local_shadows
            .update(&self.queue, camera.position, &sources, dt);
    }
}
