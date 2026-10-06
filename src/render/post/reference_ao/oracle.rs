//! Direct f64 evaluation of supplied ambientOcclusion.glsl/deferred1.glsl.
use glam::{DMat4, DVec3, DVec4};
pub(super) struct Image {
    pub size: usize,
    pub depth: Vec<f64>,
    pub inverse: DMat4,
    pub frame: u32,
}
impl Image {
    fn z(&self, uv: [f64; 2]) -> f64 {
        let index = |x: f64| {
            (x * self.size as f64)
                .floor()
                .clamp(0.0, (self.size - 1) as f64) as usize
        };
        self.depth[index(uv[1]) * self.size + index(uv[0])]
    }
    fn position(&self, uv: [f64; 2]) -> DVec3 {
        let p = self.inverse * DVec4::new(2.0 * uv[0] - 1.0, 1.0 - 2.0 * uv[1], self.z(uv), 1.0);
        p.truncate() / p.w
    }
    fn linear(&self, uv: [f64; 2]) -> f64 {
        -self.position(uv).z
    }
    // Keep literal GLSL6.28/0.7071; exact mathematical constants would change
    // the source equation and its independent regression oracle.
    #[allow(clippy::approx_constant)]
    pub fn sample(&self, x: usize, y: usize) -> f64 {
        let uv = [
            (x as f64 + 0.5) / self.size as f64,
            (y as f64 + 0.5) / self.size as f64,
        ];
        if self.z(uv) >= 1.0 {
            return 1.0;
        }
        let pixel = 1.0 / self.size as f64;
        let center = self.position(uv);
        let linear = -center.z;
        let e = self.position([uv[0] + pixel, uv[1]]);
        let w = self.position([uv[0] - pixel, uv[1]]);
        let n = self.position([uv[0], uv[1] - pixel]);
        let s = self.position([uv[0], uv[1] + pixel]);
        let h = if (-e.z - linear).abs() < (-w.z - linear).abs() {
            e - center
        } else {
            center - w
        };
        let v = if (-n.z - linear).abs() < (-s.z - linear).abs() {
            n - center
        } else {
            center - s
        };
        let cross = h.cross(v);
        let normal = if cross.length_squared() < 1e-20 {
            (-center).normalize()
        } else {
            cross.normalize()
        };
        let dither = (192.0 / 255.0 + f64::from(self.frame) * 0.618).fract();
        let mut step = 0.2475 * dither + 0.01;
        let mut dir = [(dither * 6.28).cos(), (dither * 6.28).sin()];
        let scale = 0.25 / (linear.max(2.5) * 1.37); // fixture FOV=pi/2, square viewport
        let threshold = 0.15 + linear * 0.01;
        let mut ao = 0.0;
        let mut pointiness = 0.0;
        for _ in 0..4 {
            let mut visibility = 0.0;
            for sign in [1.0, -1.0] {
                let sample = [
                    uv[0] + sign * dir[0] * step * scale,
                    uv[1] - sign * dir[1] * step * scale,
                ];
                let diff =
                    (self.position(sample) - center) / (0.25 * step * linear / linear.max(2.5));
                let angle = normal.dot(diff.normalize_or_zero()) * (1.0 + threshold);
                let attenuation = (1.0 + 0.5 / step - 0.25 * diff.length()).clamp(0.0, 1.0);
                visibility += 0.5 - (angle - threshold).max(0.0) * attenuation;
                pointiness += (-angle - threshold).max(0.0);
            }
            ao += visibility.clamp(0.0, 1.0);
            step += 0.2475;
            dir = [(dir[0] - dir[1]) * 0.7071, (dir[0] + dir[1]) * 0.7071];
        }
        let a = ao * 0.25;
        let p = pointiness * 0.25;
        ((a + (1.0 - a) * p).clamp(0.0, 1.0) * 255.0).round() / 255.0
    }
    pub fn reconstruct(&self, x: usize, y: usize, values: &[f64]) -> f64 {
        let uv = [
            (x as f64 + 0.5) / self.size as f64,
            (y as f64 + 0.5) / self.size as f64,
        ];
        if self.z(uv) >= 1.0 {
            return 1.0;
        }
        let sample = |u: [f64; 2]| {
            let px = [u[0] * self.size as f64 - 0.5, u[1] * self.size as f64 - 0.5];
            let base = [px[0].floor(), px[1].floor()];
            let f = [px[0] - base[0], px[1] - base[1]];
            let mut total = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let ix = (base[0] + dx as f64).clamp(0.0, (self.size - 1) as f64) as usize;
                    let iy = (base[1] + dy as f64).clamp(0.0, (self.size - 1) as f64) as usize;
                    total += values[iy * self.size + ix]
                        * if dx == 0 { 1.0 - f[0] } else { f[0] }
                        * if dy == 0 { 1.0 - f[1] } else { f[1] };
                }
            }
            total
        };
        let taps = [[1.5, 0.5], [-0.5, 1.5], [-1.5, -0.5], [0.5, -1.5]];
        let depths = [[2.0, 1.0], [-1.0, 2.0], [-2.0, -1.0], [1.0, -2.0]];
        let mut total = 0.0;
        let mut weight = 0.0;
        for i in 0..4 {
            let offset = |p: [f64; 2]| {
                [
                    uv[0] + p[0] / self.size as f64,
                    uv[1] - p[1] / self.size as f64,
                ]
            };
            let w =
                (1.0 - 4.0 * (self.linear(uv) - self.linear(offset(depths[i]))).abs()).max(0.00001);
            total += sample(offset(taps[i])) * w;
            weight += w;
        }
        if weight < 0.0001 {
            sample(uv)
        } else {
            total / weight
        }
    }
}
