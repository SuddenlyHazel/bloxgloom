//! Explicit diagnostic accumulation; zero preserves interactive history limits.
pub(super) fn configured_samples() -> u32 {
    parse_samples(
        std::env::var("BLOXGLOOM_GI_HISTORY_SAMPLES")
            .ok()
            .as_deref(),
    )
}

fn parse_samples(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|samples| (32..=256).contains(samples))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
