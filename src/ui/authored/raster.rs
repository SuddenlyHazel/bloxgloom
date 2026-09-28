//! Bounded decoding/rasterization, never called by a client frame.
use super::super::UiRect;
use super::*;
mod font_limits;

#[derive(Debug)]
pub(super) struct Glyph {
    pub(super) uv: UiRect,
    pub(super) offset: [f32; 2],
    pub(super) advance: f32,
}

pub(super) struct Atlas {
    pub(super) pixels: Vec<u8>,
    x: usize,
    y: usize,
    row_height: usize,
}

impl Atlas {
    pub(super) fn new() -> Self {
        Self {
            pixels: vec![0; ATLAS_SIZE * ATLAS_SIZE * 4],
            x: 0,
            y: 0,
            row_height: 0,
        }
    }

    fn insert(&mut self, width: usize, height: usize, rgba: &[u8]) -> Result<UiRect> {
        if width > 256 || height > 256 {
            return Err(INVALID);
        }
        if self.x + width + 1 > ATLAS_SIZE {
            self.x = 0;
            self.y += self.row_height;
            self.row_height = 0;
        }
        if self.y + height + 1 > ATLAS_SIZE {
            return Err(INVALID);
        }
        for y in 0..height {
            let start = ((self.y + y) * ATLAS_SIZE + self.x) * 4;
            self.pixels[start..start + width * 4]
                .copy_from_slice(&rgba[y * width * 4..(y + 1) * width * 4]);
        }
        let rect = UiRect {
            x: self.x as f32,
            y: self.y as f32,
            width: width as f32,
            height: height as f32,
        };
        self.x += width + 1;
        self.row_height = self.row_height.max(height + 1);
        Ok(rect)
    }

    pub(super) fn image(&mut self, bytes: &[u8], total_pixels: &mut usize) -> Result<UiRect> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_limits(png::Limits { bytes: 1024 * 1024 });
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|_| INVALID)?;
        let info = reader.info();
        if info.width == 0
            || info.height == 0
            || info.width > 256
            || info.height > 256
            || info.animation_control.is_some()
        {
            return Err(INVALID);
        }
        *total_pixels += info.width as usize * info.height as usize;
        if *total_pixels > 262_144 {
            return Err(INVALID);
        }
        let size = reader.output_buffer_size().ok_or(INVALID)?;
        if size > 256 * 256 * 4 {
            return Err(INVALID);
        }
        let mut pixels = vec![0; size];
        let info = reader.next_frame(&mut pixels).map_err(|_| INVALID)?;
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        for pixel in pixels[..info.buffer_size()].chunks_exact(info.color_type.samples()) {
            rgba.extend_from_slice(&match info.color_type {
                png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
                png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
                png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
                _ => return Err(INVALID),
            });
        }
        self.insert(info.width as usize, info.height as usize, &rgba)
    }

    pub(super) fn font(&mut self, bytes: &[u8]) -> Result<Vec<Glyph>> {
        // fontdue builds per-glyph outlines. Bound that expansion before loading.
        let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| INVALID)?;
        font_limits::validate(&face)?;
        let font = fontdue::Font::from_bytes(
            bytes,
            fontdue::FontSettings {
                load_substitutions: false,
                scale: 20.0,
                ..Default::default()
            },
        )
        .map_err(|_| INVALID)?;
        let mut glyphs = Vec::with_capacity(95);
        for ch in b' '..=b'~' {
            let metrics = font.metrics(char::from(ch), 20.0);
            if metrics.width > 48
                || metrics.height > 48
                || !metrics.advance_width.is_finite()
                || !(0.0..=48.0).contains(&metrics.advance_width)
            {
                return Err(INVALID);
            }
            let (_, mask) = font.rasterize(char::from(ch), 20.0);
            let mut rgba = Vec::with_capacity(mask.len() * 4);
            for alpha in mask {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            glyphs.push(Glyph {
                uv: self.insert(metrics.width, metrics.height, &rgba)?,
                offset: [
                    metrics.xmin as f32,
                    22.0 - metrics.ymin as f32 - metrics.height as f32,
                ],
                advance: metrics.advance_width,
            });
        }
        Ok(glyphs)
    }
}
