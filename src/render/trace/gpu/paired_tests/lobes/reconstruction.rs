//! Independent float32 raw/moment oracle beside the actual three-mode host draw.
use super::*;

pub(super) struct Oracle {
    size: wgpu::Extent3d,
    moments: Vec<[f32; 4]>,
    ages: Vec<f32>,
    discriminating: usize,
}

fn raw_layer(f: &Fixture, raw: &wgpu::TextureView, layer: u32) -> Vec<[f32; 4]> {
    let view = raw.texture().create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_array_layer: layer,
        array_layer_count: Some(1),
        ..Default::default()
    });
    super::probe::read(f, &view)
}

impl Oracle {
    pub(super) fn new(size: wgpu::Extent3d) -> Self {
        let pixels = (size.width * size.height) as usize;
        Self {
            size,
            moments: vec![[0.0; 4]; pixels],
            ages: vec![0.0; pixels],
            discriminating: 0,
        }
    }
    pub(super) fn observe(
        &mut self,
        f: &Fixture,
        gpu: &Gpu,
        views: [&wgpu::TextureView; 4],
        frame: u32,
        history_available: bool,
    ) {
        let reconstruction = gpu.water_reconstruction.as_ref().unwrap();
        let current = (gpu.frame as usize - 1) % 2;
        let raw = raw_layer(f, &reconstruction.raw, 0);
        let reflected = raw_layer(f, &reconstruction.raw, 1);
        let metadata = raw_layer(f, &reconstruction.raw, 2);
        let actual = super::probe::read(f, &reconstruction.moments[current]);
        let guides = super::probe::read(f, &reconstruction.guide[current]);
        let geometry = super::probe::read(f, &gpu.history_geometry[current]);
        let mean = super::probe::read(f, &gpu.history[current]);
        let previous_moments = self.moments.clone();
        let previous_ages = self.ages.clone();
        for pixel in 0..raw.len() {
            assert!(
                reflected[pixel][3] >= 0.0,
                "actual air-facing horizontal water guide"
            );
            let lum = |rgb: [f32; 3]| rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            let r = lum([
                reflected[pixel][0],
                reflected[pixel][1],
                reflected[pixel][2],
            ]);
            let t = lum(std::array::from_fn(|c| raw[pixel][c] - reflected[pixel][c]));
            let mut expected = [r, r * r, t, t * t];
            let mut age = 1.0;
            let hp = &metadata[pixel][2..];
            let x = pixel as u32 % self.size.width;
            let y = pixel as u32 / self.size.width;
            let expected_hp = if !history_available {
                [-1.0, -1.0]
            } else {
                [x as f32, y as f32]
            };
            assert_eq!(
                hp, expected_hp,
                "exact accepted primary hp frame{frame} pixel{pixel}"
            );
            if hp[0] >= 0.0 && hp[1] >= 0.0 {
                let old = (hp[1] as u32 * self.size.width + hp[0] as u32) as usize;
                if previous_ages[old] > 0.0 {
                    age = geometry[pixel][3].min(previous_ages[old] + 1.0);
                    let blend = 1.0 - 1.0 / age;
                    for c in 0..4 {
                        expected[c] =
                            expected[c] * (1.0 - blend) + previous_moments[old][c] * blend;
                    }
                }
            }
            for c in 0..4 {
                assert!(
                    (actual[pixel][c] - expected[c]).abs() <= 1e-5 * expected[c].abs().max(1.0),
                    "true raw moment frame{frame} pixel{pixel} c{c}: {} != {}",
                    actual[pixel][c],
                    expected[c]
                );
            }
            assert_eq!(
                guides[pixel].map(f32::to_bits),
                [
                    metadata[pixel][0],
                    metadata[pixel][1],
                    reflected[pixel][3],
                    age
                ]
                .map(f32::to_bits)
            );
            assert_eq!(
                geometry[pixel][3], age,
                "original supported history age reused"
            );
            self.moments[pixel] = expected;
            self.ages[pixel] = age;
            if frame > 0
                && (0..3).any(|c| {
                    (raw[pixel][c] - mean[pixel][c]).abs() > 0.005 * raw[pixel][c].abs().max(1.0)
                })
            {
                self.discriminating += 1;
            }
        }
        if frame == 0 || frame == 9 {
            self.independent_primary_probe(f, gpu, views, &raw, &reflected, &metadata);
        }
    }
    fn independent_primary_probe(
        &self,
        f: &Fixture,
        gpu: &Gpu,
        views: [&wgpu::TextureView; 4],
        raw: &[[f32; 4]],
        reflected: &[[f32; 4]],
        metadata: &[[f32; 4]],
    ) {
        let source = shaders::transport_for_lobes(true);
        let anchor = "    return RayTransportOutput(vec4f(delta,receiver.w),";
        assert_eq!(source.matches(anchor).count(), 1);
        for (field, expected) in [("radiance", raw), ("reflection", reflected)] {
            let candidate = if field == "reflection" {
                source.replace(
                    anchor,
                    "    return RayTransportOutput(vec4f(reflection,receiver.w),",
                )
            } else {
                source.clone()
            };
            eprintln!("water oracle: independent unaccumulated {field}");
            let actual = super::probe::primary(f, gpu, views, &candidate, self.size);
            for (pixel, value) in actual.iter().enumerate() {
                for c in 0..3 {
                    assert!(
                        (value[c] - expected[pixel][c]).abs() <= 2e-5 * value[c].abs().max(1.0),
                        "raw capture matches unaccumulated production {field} pixel{pixel} c{c}: {} != {}",
                        expected[pixel][c],
                        value[c]
                    );
                }
            }
        }
        let source = source.replace(
            anchor,
            "    return RayTransportOutput(vec4f(oct_encode(n),receiver.z,0.0),",
        );
        eprintln!("water oracle: independent mapped guide");
        let actual = super::probe::primary(f, gpu, views, &source, self.size);
        for (pixel, value) in actual.iter().enumerate() {
            for (c, expected) in [metadata[pixel][0], metadata[pixel][1], reflected[pixel][3]]
                .into_iter()
                .enumerate()
            {
                assert!(
                    (value[c] - expected).abs() <= 1e-5,
                    "actual primary mapped guide pixel{pixel} c{c}: {expected} != {}",
                    value[c]
                );
            }
        }
    }
    pub(super) fn assert_discriminates_raw_from_mean(&self) {
        assert!(
            self.discriminating >= 12,
            "raw oracle must distinguish raw samples from already accumulated means"
        );
    }
}
