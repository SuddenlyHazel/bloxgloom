//! Local-only comparison input: BSL's supplied noise is not redistributable.
//! Nothing from the user's shader checkout is embedded in the executable.
use std::{io::Cursor, path::PathBuf};
use wgpu::util::DeviceExt;
pub(super) const SHADER: &str = include_str!("reference_clouds.wgsl");
pub(crate) struct Noise {
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    pending: Option<wgpu::Buffer>,
    pub(crate) available: bool,
    size: (u32, u32),
}
impl Noise {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let reference = super::super::bsl_reference::enabled();
        let path = std::env::var_os("BLOXGLOOM_BSL_NOISE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("external/shaders/tex/noise.png"));
        let image = reference.then(|| Self::read(&path));
        let available = matches!(image, Some(Ok(_)));
        let (size, pixels) = match image {
            Some(Ok(image)) => image,
            Some(Err(error)) => {
                eprintln!(
                    "BSL reference noise unavailable: {}: {error}; using enhanced clouds and omitting source film grain. Supply local noise with BLOXGLOOM_BSL_NOISE (BSL author redistribution permission is required).",
                    path.display()
                );
                ((1, 1), vec![0; 4])
            }
            None => ((1, 1), vec![0; 4]),
        };
        Self::image(device, size, pixels, available)
    }

    #[cfg(test)]
    pub(crate) fn fixture(device: &wgpu::Device, rgba: [u8; 4]) -> Self {
        Self::image(device, (512, 512), rgba.repeat(512 * 512), true)
    }

    fn image(device: &wgpu::Device, size: (u32, u32), pixels: Vec<u8>, available: bool) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("user supplied BSL linear noise"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("source noise blur repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pending = available.then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("local BSL noise upload"),
                contents: &pixels,
                usage: wgpu::BufferUsages::COPY_SRC,
            })
        });
        Self {
            texture,
            view,
            sampler,
            pending,
            available,
            size,
        }
    }
    fn read(path: &std::path::Path) -> Result<((u32, u32), Vec<u8>), String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut decoder = png::Decoder::new(Cursor::new(data));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("noise output size")?];
        let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
        if (info.width, info.height, info.color_type) != (512, 512, png::ColorType::Rgba) {
            return Err("expected source 512×512 RGBA noise".into());
        }
        pixels.truncate(info.buffer_size());
        Ok(((info.width, info.height), pixels))
    }
    pub(crate) fn upload(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(buffer) = self.pending.take() {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.size.0 * 4),
                        rows_per_image: Some(self.size.1),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.size.0,
                    height: self.size.1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
#[cfg(test)]
#[path = "reference_clouds/tests.rs"]
mod tests;
