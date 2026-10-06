//! Linear numeric evidence accompanies the shared fixed-curve preview images.
use super::Snapshot;
const NAMES: [&str; 6] = [
    "legacy-combined",
    "legacy-reflection",
    "legacy-remainder",
    "optical-combined",
    "optical-reflection",
    "optical-remainder",
];
impl Snapshot {
    pub(in crate::render::trace) fn write(
        &self,
        directory: &std::path::Path,
    ) -> Result<String, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(directory)?;
        let mut summaries = Vec::new();
        let selected = self.guide.iter().map(|g| g[3] > 0.0).collect::<Vec<_>>();
        let supported = selected.iter().filter(|enabled| **enabled).count();
        for (name, pixels) in NAMES.iter().zip(&self.outputs) {
            std::fs::write(
                directory.join(format!("{name}.rgba32f")),
                bytemuck::cast_slice(pixels),
            )?;
            let preview = pixels
                .iter()
                .flat_map(|pixel| {
                    let rgb: [u8; 3] = std::array::from_fn(|c| {
                        let linear = pixel[c].max(0.0);
                        ((linear / (1.0 + linear)).powf(1.0 / 2.2) * 255.0).round() as u8
                    });
                    [rgb[0], rgb[1], rgb[2], 255]
                })
                .collect::<Vec<_>>();
            let mut png = png::Encoder::new(
                std::fs::File::create(directory.join(format!("{name}.png")))?,
                self.width,
                self.height,
            );
            png.set_color(png::ColorType::Rgba);
            png.set_depth(png::BitDepth::Eight);
            png.write_header()?.write_image_data(&preview)?;
            let mut mean = [0.0f64; 3];
            let mut peak = 0.0f32;
            let mut nonfinite = 0;
            let mut local_square = 0.0f64;
            let mut edges = 0u64;
            for (i, p) in pixels.iter().enumerate() {
                if !selected[i] {
                    continue;
                }
                if !p[..3].iter().all(|v| v.is_finite()) {
                    nonfinite += 1;
                    continue;
                }
                for c in 0..3 {
                    mean[c] += f64::from(p[c]);
                    peak = peak.max(p[c]);
                }
                for neighbor in [
                    i.checked_sub(1).filter(|_| i % self.width as usize > 0),
                    i.checked_sub(self.width as usize),
                ]
                .into_iter()
                .flatten()
                {
                    if selected[neighbor] {
                        for c in 0..3 {
                            local_square += f64::from(p[c] - pixels[neighbor][c]).powi(2);
                        }
                        edges += 1;
                    }
                }
            }
            for channel in &mut mean {
                *channel /= supported.max(1) as f64;
            }
            summaries.push(serde_json::json!({"name":name,"supported_mean_rgb":mean,"supported_peak":peak,"nonfinite":nonfinite,"neighbor_squared_difference":local_square/edges.max(1) as f64}));
        }
        let mut errors = [0.0f64; 2];
        for (mode, error) in errors.iter_mut().enumerate() {
            let at = mode * 3;
            for (i, enabled) in selected.iter().enumerate() {
                if *enabled {
                    for c in 0..3 {
                        *error = error.max(f64::from(
                            (self.outputs[at][i][c]
                                - self.outputs[at + 1][i][c]
                                - self.outputs[at + 2][i][c])
                                .abs(),
                        ));
                    }
                }
            }
        }
        std::fs::write(
            directory.join("guide.rgba32f"),
            bytemuck::cast_slice(&self.guide),
        )?;
        let report = serde_json::json!({"width":self.width,"height":self.height,"supported_pixels":supported,
            "notes":"Same cached path samples; linear float32 files are little-endian RGBA row-major. PNGs share fixed x/(1+x), gamma2.2. Neighbor difference is a spatial grain proxy, not Monte Carlo variance or an unbiased mean proof. Signed dynamic correction and raster baseline are excluded.",
            "split_sum_max_absolute_error":errors,"outputs":summaries});
        let report = serde_json::to_string_pretty(&report)?;
        std::fs::write(directory.join("metrics.json"), &report)?;
        Ok(format!(
            "cached water filter diagnostics: {supported} supported pixels, {}",
            directory.display()
        ))
    }
}
