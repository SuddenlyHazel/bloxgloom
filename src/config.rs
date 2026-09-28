//! Versioned local settings for the desktop client.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::{MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE};
pub(crate) mod bindings;
use bindings::Bindings;

const CONFIG_VERSION: u32 = 1;
const MIN_SENSITIVITY: f32 = 0.0002;
const MAX_SENSITIVITY: f32 = 0.01;
const MIN_FOV: f32 = 40.0;
const MAX_FOV: f32 = 110.0;
const MIN_SCALE: f32 = 0.75;
const MAX_SCALE: f32 = 2.0;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub sensitivity: f32,
    pub fov_degrees: f32,
    pub view_distance: u8,
    pub scale: f32,
    pub fullscreen: bool,
    pub bounced_gi: bool,
    pub exposure: f32,
    pub post_processing: bool,
    pub bloom_enabled: bool,
    pub bloom_strength: f32,
    pub selected_slot: usize,
    pub debug_hud: bool,
    pub profile: u128,
    pub(crate) bindings: Bindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale: 1.0,
            fullscreen: false,
            bounced_gi: false,
            exposure: 1.0,
            post_processing: true,
            bloom_enabled: true,
            bloom_strength: 0.12,
            selected_slot: 1,
            debug_hud: false,
            profile: 0,
            bindings: Bindings::default(),
        }
    }
}

impl Config {
    /// Returns the native per-user settings path. Callers can use `load` and
    /// `save` with another path to support an explicit override.
    pub fn default_path() -> PathBuf {
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
            return base.join("Bloxgloom/config");
        }

        #[cfg(target_os = "linux")]
        {
            let base = nonempty_env("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| user_home().map(|home| home.join(".config")))
                .unwrap_or_else(|| PathBuf::from("."));
            return base.join("bloxgloom/config");
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
        Self {
            sensitivity: clamp_finite(self.sensitivity, MIN_SENSITIVITY, MAX_SENSITIVITY, 0.002),
            fov_degrees: clamp_finite(self.fov_degrees, MIN_FOV, MAX_FOV, 70.0),
            view_distance: self
                .view_distance
                .clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE),
            scale: clamp_finite(self.scale, MIN_SCALE, MAX_SCALE, 1.0),
            fullscreen: self.fullscreen,
            bounced_gi: self.bounced_gi,
            exposure: clamp_finite(self.exposure, 0.25, 4.0, 1.0),
            post_processing: self.post_processing,
            bloom_enabled: self.bloom_enabled,
            bloom_strength: clamp_finite(self.bloom_strength, 0.0, 1.0, 0.12),
            selected_slot: self.selected_slot.min(8),
            debug_hud: self.debug_hud,
            profile: self.profile,
            bindings: if self.bindings.valid() {
                self.bindings
            } else {
                Bindings::default()
            },
        }
    }

    fn serialize(&self) -> String {
        format!(
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
        )
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
            "version" => version = value.parse::<u32>().ok(),
            "sensitivity" => {
                config.sensitivity =
                    parse_clamped_float(value, MIN_SENSITIVITY, MAX_SENSITIVITY, 0.002);
            }
            "fov_degrees" => {
                config.fov_degrees = parse_clamped_float(value, MIN_FOV, MAX_FOV, 70.0);
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
