//! Exact, measured optimizations within the separately opt-in path tracer.
//! Explicit zero retains the reference path for headless comparisons.
fn enabled(value: Option<&str>) -> bool {
    value != Some("0")
}

pub(super) fn material_fast() -> bool {
    enabled(std::env::var("BLOXGLOOM_GI_MATERIAL_FAST").ok().as_deref())
}

pub(super) fn tight_bounds() -> bool {
    enabled(std::env::var("BLOXGLOOM_GI_TIGHT_BVH").ok().as_deref())
}

#[cfg(test)]
mod tests;
