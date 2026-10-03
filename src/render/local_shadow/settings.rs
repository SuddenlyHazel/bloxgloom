//! Startup limits keep map storage and six-face work explicitly bounded.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Zero disables all local maps.
    pub count: usize,
    pub resolution: u32,
    pub range: f32,
    /// Whole lights refreshed per frame, each requiring six depth passes.
    pub updates: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            count: 2,
            resolution: 256,
            range: 16.0,
            updates: 2,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.count > 4 {
            return Err("local shadow count must be 0..=4".into());
        }
        if !(64..=1024).contains(&self.resolution) {
            return Err("local shadow resolution must be 64..=1024".into());
        }
        if !self.range.is_finite() || !(2.0..=32.0).contains(&self.range) {
            return Err("local shadow range must be finite and 2..=32".into());
        }
        if !(1..=4).contains(&self.updates) {
            return Err("local shadow updates must be 1..=4".into());
        }
        Ok(())
    }

    pub(crate) fn configured(maximum: u32) -> Self {
        Self::default().with_environment(maximum)
    }

    pub(crate) fn with_environment(mut self, maximum: u32) -> Self {
        let value = |name: &str| std::env::var(format!("BLOXGLOOM_LOCAL_SHADOW_{name}")).ok();
        if let Some(count) = value("COUNT").and_then(|s| s.parse().ok()) {
            self.count = count;
        }
        if let Some(resolution) = value("RESOLUTION").and_then(|s| s.parse().ok()) {
            self.resolution = resolution;
        }
        if let Some(range) = value("RANGE").and_then(|s| s.parse().ok()) {
            self.range = range;
        }
        if let Some(updates) = value("UPDATES").and_then(|s| s.parse().ok()) {
            self.updates = updates;
        }
        if let Ok(enabled) = std::env::var("BLOXGLOOM_LOCAL_SHADOWS") {
            if matches!(enabled.as_str(), "0" | "off" | "false") {
                self.count = 0;
            } else if self.count == 0 {
                self.count = 2;
            }
        }
        self.sanitized(maximum)
    }

    pub(crate) fn sanitized(mut self, maximum: u32) -> Self {
        self.count = self.count.min(4);
        self.resolution = if self.count == 0 {
            1
        } else {
            self.resolution.clamp(64, 1024).min(maximum).max(1)
        };
        self.range = if self.range.is_finite() {
            self.range.clamp(2.0, 32.0)
        } else {
            16.0
        };
        self.updates = self.updates.clamp(1, self.count.max(1));
        self
    }
}
