//! CPU summary of explicit headless, prefilter linear-HDR readback.
pub(super) fn summarize(samples: &[[f32; 4]], width: u32, solar: f32) -> String {
    let mut values = Vec::new();
    let mut rgb_sum = [0.0f64; 3];
    let mut peaks = Vec::new();
    let mut thresholds = [0usize; 3];
    for (index, pixel) in samples.iter().enumerate() {
        if pixel[3] == 0.0 || pixel[..3].iter().any(|value| !value.is_finite()) {
            continue;
        }
        let luminance = pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722;
        values.push(luminance);
        for (sum, value) in rgb_sum.iter_mut().zip(&pixel[..3]) {
            *sum += f64::from(*value);
        }
        if solar > 0.0 {
            for (i, threshold) in [1.0, 10.0, 100.0].into_iter().enumerate() {
                thresholds[i] += usize::from(luminance > solar * threshold);
            }
        }
        peaks.push((index, [pixel[0], pixel[1], pixel[2]]));
    }
    for value in &mut rgb_sum {
        *value /= values.len().max(1) as f64;
    }
    values.sort_unstable_by(f32::total_cmp);
    let percentile = |percent: usize| {
        values
            .get(values.len().saturating_sub(1) * percent / 1000)
            .copied()
            .unwrap_or(0.0)
    };
    peaks.sort_unstable_by(|a, b| {
        let peak = |rgb: &[f32; 3]| rgb.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        peak(&b.1).total_cmp(&peak(&a.1))
    });
    let peaks: Vec<_> = peaks
        .into_iter()
        .take(16)
        .map(|(index, rgb)| (index % width as usize, index / width as usize, rgb))
        .collect();
    format!(
        "prefilter mean-RGB={rgb_sum:?} luminance-p99={:.6} p99.9={:.6} max={:.6} solar-luminance={solar:.6} above-1/10/100-solar={thresholds:?} top-max-channel-peaks(x,y,RGB)={peaks:?}",
        percentile(990),
        percentile(999),
        percentile(1000)
    )
}
