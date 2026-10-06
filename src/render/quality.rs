//! World render resolution is independent of native presentation and UI size.
use super::Renderer;

pub(crate) fn dimensions(width: u32, height: u32, scale: f32) -> (u32, u32) {
    let scale = if scale.is_finite() {
        scale.clamp(crate::config::quality::MIN_RENDER_SCALE, 1.0)
    } else {
        1.0
    };
    (
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )
}

impl Renderer {
    pub(crate) fn configure_quality(&mut self, scale: f32, reflections: bool) {
        let scale = if scale.is_finite() {
            scale.clamp(crate::config::quality::MIN_RENDER_SCALE, 1.0)
        } else {
            1.0
        };
        self.post.reflections.set_enabled(reflections);
        if self.render_scale != scale {
            self.render_scale = scale;
            self.post.configure_reduced_resolution(scale < 1.0);
            self.resize_scene();
        }
    }

    pub(super) fn resize_scene(&mut self) {
        let (width, height) = dimensions(self.config.width, self.config.height, self.render_scale);
        let previous = self.post.scene.texture().size();
        if (previous.width, previous.height) == (width, height) {
            return;
        }
        self.depth = super::create_depth(&self.device, width, height);
        self.post.resize(&self.device, width, height);
        tracing::info!(
            window_width = self.config.width,
            window_height = self.config.height,
            render_width = width,
            render_height = height,
            scale = self.render_scale,
            "world render resolution"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::dimensions;
    #[test]
    fn retina_scaling_reduces_world_pixels_without_changing_native_extent() {
        assert_eq!(dimensions(3456, 2234, 0.35), (1210, 782));
        assert_eq!(dimensions(3456, 2234, 0.5), (1728, 1117));
        assert_eq!(dimensions(3456, 2234, 1.0), (3456, 2234));
        assert_eq!(dimensions(1281, 721, 0.67), (858, 483));
    }
    #[test]
    fn minimized_and_invalid_dimensions_remain_valid_attachments() {
        assert_eq!(dimensions(0, 0, 0.5), (1, 1));
        assert_eq!(dimensions(1, 1, 0.5), (1, 1));
        assert_eq!(dimensions(10, 10, f32::NAN), (10, 10));
        assert_eq!(dimensions(10, 10, 0.0), (4, 4));
    }
}
