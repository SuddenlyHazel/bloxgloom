//! Explicit preview-only filter readback; ordinary frames allocate nothing here.
use super::TraceLighting;
pub(crate) struct WaterFilterDiagnostics {
    pub summary: String,
    pub hdr: Option<[wgpu::TextureView; 2]>,
}
impl TraceLighting {
    pub fn write_water_filter_diagnostics(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        views: [&wgpu::TextureView; 4],
        directory: &std::path::Path,
        full_frame: bool,
    ) -> Result<Option<WaterFilterDiagnostics>, Box<dyn std::error::Error>> {
        let Some(gpu) = &self.gpu else {
            return Ok(None);
        };
        gpu.cached_water_filters(device, queue, views, full_frame)?
            .map(|snapshot| {
                Ok(WaterFilterDiagnostics {
                    summary: snapshot.write(directory)?,
                    hdr: snapshot.full_frame,
                })
            })
            .transpose()
    }
}
