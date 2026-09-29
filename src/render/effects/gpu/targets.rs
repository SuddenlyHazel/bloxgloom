// At most 64 MiB of intermediate HDR textures. Preserve aspect ratio while
// capping the largest extent to 2048 and scaling all passes together to budget.
pub(super) fn target_sizes(
    width: u32,
    height: u32,
    device_limit: u32,
    scales: &[u8],
) -> Vec<(u32, u32)> {
    let limit = 2048u32.min(device_limit).max(1);
    let mut divisor = width.max(height).max(1).div_ceil(limit);
    loop {
        let sizes = scales
            .iter()
            .map(|&scale| {
                let divisor = divisor.saturating_mul(u32::from(scale));
                (
                    width.div_ceil(divisor).max(1),
                    height.div_ceil(divisor).max(1),
                )
            })
            .collect::<Vec<_>>();
        if sizes
            .iter()
            .map(|&(w, h)| u64::from(w) * u64::from(h) * 8)
            .sum::<u64>()
            <= 64 * 1024 * 1024
        {
            return sizes;
        }
        divisor += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_remain_bounded_at_large_and_tiny_viewports() {
        for (w, h) in [(1, 1), (7680, 4320), (u32::MAX, u32::MAX)] {
            let sizes = target_sizes(w, h, 4096, &[1; crate::render::effects::MAX_PASSES]);
            assert!(
                sizes
                    .iter()
                    .all(|&(w, h)| w > 0 && h > 0 && w <= 2048 && h <= 2048)
            );
            assert!(
                sizes
                    .iter()
                    .map(|&(w, h)| u64::from(w) * u64::from(h) * 8)
                    .sum::<u64>()
                    <= 64 * 1024 * 1024
            );
        }
    }
}
