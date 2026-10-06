//! Versioned local settings for the desktop client.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::{MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE};
pub(crate) mod bindings;
pub(crate) mod lighting;
pub(crate) mod parallax;
pub(crate) mod quality;
use bindings::{Bindings, NamedBindings};

const CONFIG_VERSION: u32 = 1;
const MIN_SENSITIVITY: f32 = 0.0002;
const MAX_SENSITIVITY: f32 = 0.01;
const MIN_FOV: f32 = 40.0;
const MAX_FOV: f32 = 110.0;
const MIN_SCALE: f32 = 0.75;
const MAX_SCALE: f32 = 2.0;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Local quality choice for directional sun shadows, independent of voxel lighting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SunShadowQuality {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

impl SunShadowQuality {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    pub const fn cycle(self, increase: bool) -> Self {
        match (self, increase) {
            (Self::Off, true) | (Self::Medium, false) => Self::Low,
            (Self::Low, true) | (Self::High, false) => Self::Medium,
            (Self::Medium, true) | (Self::Off, false) => Self::High,
            (Self::High, true) | (Self::Low, false) => Self::Off,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub sensitivity: f32,
    pub fov_degrees: f32,
    pub view_distance: u8,
    /// Zero disables distant terrain; supported horizons are 512 and 1024 blocks.
    pub lod_horizon: u16,
    /// 0 coarse, 1 balanced, 2 detailed.
    pub lod_quality: u8,
    pub scale: f32,
    pub quality_preset: quality::QualityPreset,
    /// World resolution relative to physical pixels; UI always stays native.
    pub render_scale: f32,
    pub reflections_enabled: bool,
    pub fullscreen: bool,
    pub bounced_gi: bool,
    pub sun_shadow_quality: SunShadowQuality,
    pub parallax: parallax::Parallax,
    pub lighting: lighting::Lighting,
    pub local_shadows: crate::render::local_shadow::Settings,
    pub exposure: f32,
    pub post_processing: bool,
    pub bloom_enabled: bool,
    pub bloom_strength: f32,
    pub audio_master: f32,
    pub audio_ambient: f32,
    pub audio_effects: f32,
    pub audio_mix: crate::audio::mix_tuning::MixConfig,
    pub rain_audio: crate::audio::rain_tuning::RainConfig,
    pub selected_slot: usize,
    pub debug_hud: bool,
    pub profile: u128,
    pub(crate) bindings: Bindings,
    pub(crate) named_bindings: NamedBindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            lod_horizon: 512,
            lod_quality: 1,
            scale: 1.0,
            quality_preset: quality::QualityPreset::Custom,
            render_scale: 1.0,
            reflections_enabled: true,
            fullscreen: false,
            bounced_gi: false,
            sun_shadow_quality: SunShadowQuality::default(),
            parallax: parallax::Parallax::default(),
            lighting: lighting::Lighting::default(),
            local_shadows: Default::default(),
            exposure: 1.0,
            post_processing: true,
            bloom_enabled: true,
            bloom_strength: 0.12,
            audio_master: 1.0,
            audio_ambient: 1.0,
            audio_effects: 1.0,
            audio_mix: Default::default(),
            rain_audio: Default::default(),
            selected_slot: 1,
            debug_hud: false,
            profile: 0,
            bindings: Bindings::default(),
            named_bindings: NamedBindings::default(),
        }
    }
}

impl Config {
    /// Disposable package bytes use native cache storage, never a world save.
    pub(crate) fn package_cache_path() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        let base = user_home().map(|p| p.join("Library/Caches/Bloxgloom"));
        #[cfg(target_os = "windows")]
        let base = nonempty_env("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| user_home().map(|p| p.join("AppData/Local")))
            .map(|p| p.join("Bloxgloom/Cache"));
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let base = nonempty_env("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| user_home().map(|p| p.join(".cache")))
            .map(|p| p.join("bloxgloom"));
        base.map(|p| p.join("package-bundles-v1"))
    }

    /// Returns the native per-user settings path. Callers can use `load` and
    /// `save` with another path, or set `BLOXGLOOM_CONFIG`, for isolated sessions.
    pub fn default_path() -> PathBuf {
        if let Some(path) = nonempty_env("BLOXGLOOM_CONFIG") {
            return PathBuf::from(path);
        }
        #[cfg(target_os = "macos")]
        {
            user_home()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Library/Application Support/Bloxgloom/config")
        }

        #[cfg(target_os = "windows")]
        {
            let base = nonempty_env("APPDATA")
                .map(PathBuf::from)
                .or_else(|| user_home().map(|home| home.join("AppData/Roaming")))
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("Bloxgloom/config")
        }

        #[cfg(target_os = "linux")]
        {
            let base = nonempty_env("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| user_home().map(|home| home.join(".config")))
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("bloxgloom/config")
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            user_home()
                .map(|home| home.join(".config/bloxgloom/config"))
                .unwrap_or_else(|| PathBuf::from("bloxgloom/config"))
        }
    }

    /// Loads settings from `path`; missing, unsupported, or malformed files
    /// fall back to defaults. Individual out-of-range values are clamped.
    pub fn load(path: impl AsRef<Path>) -> Self {
        fs::read_to_string(path)
            .ok()
            .map(|contents| parse_config(&contents))
            .unwrap_or_default()
    }

    /// Writes settings atomically using a temporary file beside the target.
    /// No disk access is performed until this method is called.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        let bytes = self.sanitized().serialize();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let file_name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "config path has no file name")
        })?;
        let (temporary_path, file) = create_temporary_file(parent, file_name)?;
        let result = write_and_replace(file, &bytes, &temporary_path, path);
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    /// Clamps values after direct edits so the in-memory settings match the
    /// ranges used by the settings controls and persisted config.
    pub fn sanitize(&mut self) {
        *self = self.sanitized();
    }

    /// Create a persistent, unguessable local identity before connecting.
    pub fn ensure_profile(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        if self.profile == 0 {
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
            self.profile = u128::from_le_bytes(bytes);
            if self.profile == 0 {
                self.profile = 1;
            }
            self.save(path)?;
        }
        Ok(())
    }

    fn sanitized(&self) -> Self {
        let mut sanitized = Self {
            sensitivity: clamp_finite(self.sensitivity, MIN_SENSITIVITY, MAX_SENSITIVITY, 0.002),
            fov_degrees: clamp_finite(self.fov_degrees, MIN_FOV, MAX_FOV, 70.0),
            view_distance: self
                .view_distance
                .clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE),
            lod_horizon: sanitize_lod_horizon(self.lod_horizon),
            lod_quality: self.lod_quality.min(2),
            scale: clamp_finite(self.scale, MIN_SCALE, MAX_SCALE, 1.0),
            quality_preset: self.quality_preset,
            render_scale: clamp_finite(self.render_scale, quality::MIN_RENDER_SCALE, 1.0, 1.0),
            reflections_enabled: self.reflections_enabled,
            fullscreen: self.fullscreen,
            bounced_gi: self.bounced_gi,
            sun_shadow_quality: self.sun_shadow_quality,
            parallax: self.parallax.sanitized(),
            lighting: self.lighting.sanitized(),
            local_shadows: sanitize_local_shadows(self.local_shadows),
            exposure: clamp_finite(self.exposure, 0.25, 4.0, 1.0),
            post_processing: self.post_processing,
            bloom_enabled: self.bloom_enabled,
            bloom_strength: clamp_finite(self.bloom_strength, 0.0, 1.0, 0.12),
            audio_master: clamp_finite(self.audio_master, 0.0, 1.0, 1.0),
            audio_ambient: clamp_finite(self.audio_ambient, 0.0, 1.0, 1.0),
            audio_effects: clamp_finite(self.audio_effects, 0.0, 1.0, 1.0),
            audio_mix: self.audio_mix.sanitized(),
            rain_audio: self.rain_audio.sanitized(),
            selected_slot: self.selected_slot.min(8),
            debug_hud: self.debug_hud,
            profile: self.profile,
            bindings: if self.bindings.valid() {
                self.bindings
            } else {
                Bindings::default()
            },
            named_bindings: self.named_bindings.sanitized(if self.bindings.valid() {
                self.bindings
            } else {
                Bindings::default()
            }),
        };
        sanitized.quality_preset = sanitized.effective_quality_preset();
        sanitized
    }

    fn serialize(&self) -> String {
        let mut text = format!(
            "version={CONFIG_VERSION}\nsensitivity={}\nfov_degrees={}\nview_distance={}\nscale={}\nfullscreen={}\nbounced_gi={}\nselected_slot={}\ndebug_hud={}\nprofile={:032x}\nexposure={}\nbloom_strength={}\npost_processing={}\nbloom_enabled={}\nbind_inventory={}\nbind_kiln_input={}\nbind_kiln_fuel={}\nbind_drop={}\n",
            self.sensitivity,
            self.fov_degrees,
            self.view_distance,
            self.scale,
            self.fullscreen,
            self.bounced_gi,
            self.selected_slot,
            self.debug_hud,
            self.profile,
            self.exposure,
            self.bloom_strength,
            self.post_processing,
            self.bloom_enabled,
            bindings::letter(self.bindings.inventory).expect("sanitized inventory binding"),
            bindings::letter(self.bindings.kiln_input).expect("sanitized kiln input binding"),
            bindings::letter(self.bindings.kiln_fuel).expect("sanitized kiln fuel binding"),
            bindings::letter(self.bindings.drop).expect("sanitized drop binding"),
        );
        text.push_str(&format!(
            "sun_shadow_quality={}\n",
            self.sun_shadow_quality.as_str()
        ));
        text.push_str(&format!(
            "quality_preset={}\nrender_scale={}\nreflections_enabled={}\n",
            self.quality_preset.as_str(),
            self.render_scale,
            self.reflections_enabled
        ));
        text.push_str(&self.parallax.serialize());
        text.push_str(&self.lighting.serialize());
        text.push_str(&format!(
            "local_shadows={}\n",
            serde_json::to_string(&self.local_shadows).expect("sanitized local shadows")
        ));
        text.push_str(&format!(
            "audio_master={}\naudio_ambient={}\naudio_effects={}\n",
            self.audio_master, self.audio_ambient, self.audio_effects
        ));
        text.push_str(&format!(
            "audio_mix={}\n",
            serde_json::to_string(&self.audio_mix).expect("sanitized audio mix")
        ));
        text.push_str(&format!(
            "rain_audio={}\n",
            serde_json::to_string(&self.rain_audio).expect("sanitized rain audio")
        ));
        text.push_str(&format!(
            "lod_horizon={}\nlod_quality={}\n",
            self.lod_horizon, self.lod_quality
        ));
        for (action, key) in &self.named_bindings.0 {
            text.push_str(&format!(
                "bind_action.{action}={}\n",
                bindings::letter(*key).expect("sanitized named binding")
            ));
        }
        text
    }
}

fn parse_config(contents: &str) -> Config {
    let mut config = Config::default();
    let mut version = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if config.parallax.parse(key, value) || config.lighting.parse(key, value) {
            continue;
        }
        if let Some(action) = key.strip_prefix("bind_action.") {
            if config.named_bindings.0.len() >= 64 && !config.named_bindings.0.contains_key(action)
            {
                continue;
            }
            if let Some(key) = bindings::parse(value) {
                // Final sanitization also rejects conflicts with built-in keys.
                config.named_bindings.0.insert(action.to_owned(), key);
            }
            continue;
        }
        match key {
            "bind_inventory" => {
                if let Some(key) = bindings::parse(value) {
                    config.bindings.inventory = key;
                }
            }
            "bind_kiln_input" => {
                if let Some(key) = bindings::parse(value) {
                    config.bindings.kiln_input = key;
                }
            }
            "bind_kiln_fuel" => {
                if let Some(key) = bindings::parse(value) {
                    config.bindings.kiln_fuel = key;
                }
            }
            "bind_drop" => {
                if let Some(key) = bindings::parse(value) {
                    config.bindings.drop = key;
                }
            }
            "quality_preset" => {
                if let Some(preset) = quality::QualityPreset::parse(value) {
                    config.quality_preset = preset;
                }
            }
            "render_scale" => {
                config.render_scale =
                    parse_clamped_float(value, quality::MIN_RENDER_SCALE, 1.0, 1.0)
            }
            "reflections_enabled" => {
                if let Ok(enabled) = value.parse() {
                    config.reflections_enabled = enabled;
                }
            }
            "post_processing" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    config.post_processing = enabled;
                }
            }
            "bloom_enabled" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    config.bloom_enabled = enabled;
                }
            }
            "exposure" => config.exposure = parse_clamped_float(value, 0.25, 4.0, 1.0),
            "bloom_strength" => config.bloom_strength = parse_clamped_float(value, 0.0, 1.0, 0.12),
            "audio_master" => config.audio_master = parse_clamped_float(value, 0.0, 1.0, 1.0),
            "audio_ambient" => config.audio_ambient = parse_clamped_float(value, 0.0, 1.0, 1.0),
            "audio_effects" => config.audio_effects = parse_clamped_float(value, 0.0, 1.0, 1.0),
            "local_shadows" => {
                if let Ok(settings) =
                    serde_json::from_str::<crate::render::local_shadow::Settings>(value)
                {
                    config.local_shadows = sanitize_local_shadows(settings);
                }
            }
            "audio_mix" => {
                if let Ok(profile) =
                    serde_json::from_str::<crate::audio::mix_tuning::MixConfig>(value)
                {
                    config.audio_mix = profile.sanitized();
                }
            }
            "rain_audio" => {
                if let Ok(profile) =
                    serde_json::from_str::<crate::audio::rain_tuning::RainConfig>(value)
                {
                    config.rain_audio = profile.sanitized();
                }
            }
            "version" => version = value.parse::<u32>().ok(),
            "sensitivity" => {
                config.sensitivity =
                    parse_clamped_float(value, MIN_SENSITIVITY, MAX_SENSITIVITY, 0.002);
            }
            "fov_degrees" => {
                config.fov_degrees = parse_clamped_float(value, MIN_FOV, MAX_FOV, 70.0);
            }
            "lod_horizon" => {
                if let Ok(horizon) = value.parse::<u16>() {
                    config.lod_horizon = sanitize_lod_horizon(horizon);
                }
            }
            "lod_quality" => {
                if let Ok(quality) = value.parse::<u8>() {
                    config.lod_quality = quality.min(2);
                }
            }
            "view_distance" => {
                if let Ok(radius) = value.parse::<i64>() {
                    config.view_distance = radius
                        .clamp(i64::from(MIN_VIEW_DISTANCE), i64::from(MAX_VIEW_DISTANCE))
                        as u8;
                }
            }
            "scale" => config.scale = parse_clamped_float(value, MIN_SCALE, MAX_SCALE, 1.0),
            "fullscreen" => {
                if let Ok(fullscreen) = value.parse::<bool>() {
                    config.fullscreen = fullscreen;
                }
            }
            "bounced_gi" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    config.bounced_gi = enabled;
                }
            }
            "sun_shadow_quality" => {
                if let Some(quality) = SunShadowQuality::parse(value) {
                    config.sun_shadow_quality = quality;
                }
            }
            "selected_slot" => {
                if let Ok(slot) = value.parse::<i64>() {
                    config.selected_slot = slot.clamp(0, 8) as usize;
                }
            }
            "debug_hud" => {
                if let Ok(debug_hud) = value.parse::<bool>() {
                    config.debug_hud = debug_hud;
                }
            }
            "profile" => {
                if let Ok(profile) = u128::from_str_radix(value, 16) {
                    config.profile = profile;
                }
            }
            _ => {}
        }
    }

    if version == Some(CONFIG_VERSION) {
        config.sanitized()
    } else {
        Config::default()
    }
}

fn parse_clamped_float(value: &str, min: f32, max: f32, fallback: f32) -> f32 {
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(min, max))
        .unwrap_or(fallback)
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn create_temporary_file(
    parent: &Path,
    file_name: &std::ffi::OsStr,
) -> io::Result<(PathBuf, File)> {
    for _ in 0..32 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = OsString::from(file_name);
        temporary_name.push(format!(".tmp.{}.{}", std::process::id(), sequence));
        let temporary_path = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a temporary config file",
    ))
}

fn write_and_replace(
    mut file: File,
    bytes: &str,
    temporary_path: &Path,
    path: &Path,
) -> io::Result<()> {
    file.write_all(bytes.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary_path, path)
}

fn nonempty_env(key: &str) -> Option<OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

fn user_home() -> Option<PathBuf> {
    nonempty_env("HOME")
        .or_else(|| nonempty_env("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;

fn sanitize_lod_horizon(horizon: u16) -> u16 {
    match horizon {
        0 => 0,
        1..=512 => 512,
        _ => 1024,
    }
}

// Keep persistent settings in their authored bounds, even when runtime maps are disabled.
fn sanitize_local_shadows(
    settings: crate::render::local_shadow::Settings,
) -> crate::render::local_shadow::Settings {
    let mut sanitized = settings.sanitized(1024);
    sanitized.resolution = settings.resolution.clamp(64, 1024);
    sanitized
}
