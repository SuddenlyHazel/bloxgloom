//! Licensed JG RTX celestial art, unchanged source images with hardware sRGB.
use std::io::Cursor;
use wgpu::util::DeviceExt;
const SUN: &[u8] = include_bytes!("../../../assets/textures/environment/sun.png");
const MOON: &[u8] = include_bytes!("../../../assets/textures/environment/moon_phases.png");
struct Image {
    texture: wgpu::Texture,
    pending: Option<wgpu::Buffer>,
    pub(super) view: wgpu::TextureView,
    encoded_view: wgpu::TextureView,
    width: u32,
    height: u32,
}
impl Image {
    fn new(device: &wgpu::Device, bytes: &[u8], label: &str) -> Self {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("embedded celestial PNG header");
        let mut data = vec![0; reader.output_buffer_size().expect("celestial decode size")];
        let info = reader
            .next_frame(&mut data)
            .expect("embedded celestial PNG frame");
        assert_eq!(info.color_type, png::ColorType::Rgb);
        let rgba: Vec<u8> = data[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: info.width,
                height: info.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let view = texture.create_view(&Default::default());
        // GLSL filters encoded artwork before its explicit pow(rgb,2.2).
        // A second view preserves that ordering without changing the artwork.
        let encoded_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        assert_eq!((info.width * 4) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        let pending = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: &rgba,
                usage: wgpu::BufferUsages::COPY_SRC,
            }),
        );
        Self {
            texture,
            pending,
            view,
            encoded_view,
            width: info.width,
            height: info.height,
        }
    }
    fn upload(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(buffer) = self.pending.take() {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.width * 4),
                        rows_per_image: Some(self.height),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
pub(super) struct Celestial {
    sun: Image,
    moon: Image,
}
impl Celestial {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            sun: Image::new(device, SUN, "JG RTX sun artwork"),
            moon: Image::new(device, MOON, "JG RTX lunar phase artwork"),
        }
    }
    pub(super) fn sun(&self, reference: bool) -> &wgpu::TextureView {
        if reference {
            &self.sun.encoded_view
        } else {
            &self.sun.view
        }
    }
    pub(super) fn moon(&self, reference: bool) -> &wgpu::TextureView {
        if reference {
            &self.moon.encoded_view
        } else {
            &self.moon.view
        }
    }
    pub(super) fn upload(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.sun.upload(encoder);
        self.moon.upload(encoder);
    }
}

#[cfg(test)]
#[path = "celestial/tests.rs"]
mod tests;
