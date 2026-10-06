//! Optional bounded frame sequence for visual diagnostics, never timing evidence.
use std::{error::Error, path::PathBuf};

pub(super) struct Capture(Option<PathBuf>);
impl Capture {
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        let directory = std::env::var_os("BLOXGLOOM_PERF_CAPTURE_FRAMES")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        if let Some(path) = &directory {
            std::fs::create_dir_all(path)?;
            eprintln!(
                "diagnostic first-eight-steady-frame readbacks enabled; benchmark timings are invalid for performance comparison"
            );
        }
        Ok(Self(directory))
    }
    pub fn enabled(&self) -> bool {
        self.0.is_some()
    }
    pub fn save(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: &wgpu::Texture,
        index: usize,
    ) -> Result<(), Box<dyn Error>> {
        let Some(directory) = &self.0 else {
            return Ok(());
        };
        if index >= 8 {
            return Ok(());
        }
        let size = color.size();
        crate::preview::capture::save_texture(
            device,
            queue,
            color,
            size.width,
            size.height,
            &directory.join(format!("{index:02}.png")),
        )
    }
}
