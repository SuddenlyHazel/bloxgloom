//! Versioned local settings for the desktop client.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::{MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE};
use crate::world::MAX_BLOCK;

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
    pub hotbar: [u8; 9],
    pub selected_slot: usize,
    pub debug_hud: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale: 1.0,
            fullscreen: false,
            hotbar: [1, 2, 3, 4, 5, 6, 7, 8, 1],
            selected_slot: 1,
            debug_hud: false,
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

    fn sanitized(&self) -> Self {
        Self {
            sensitivity: clamp_finite(self.sensitivity, MIN_SENSITIVITY, MAX_SENSITIVITY, 0.002),
            fov_degrees: clamp_finite(self.fov_degrees, MIN_FOV, MAX_FOV, 70.0),
            view_distance: self
                .view_distance
                .clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE),
            scale: clamp_finite(self.scale, MIN_SCALE, MAX_SCALE, 1.0),
            fullscreen: self.fullscreen,
            hotbar: self.hotbar.map(|block| block.clamp(1, MAX_BLOCK)),
            selected_slot: self.selected_slot.min(8),
            debug_hud: self.debug_hud,
        }
    }

    fn serialize(&self) -> String {
        let hotbar = self
            .hotbar
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "version={CONFIG_VERSION}\nsensitivity={}\nfov_degrees={}\nview_distance={}\nscale={}\nfullscreen={}\nhotbar={hotbar}\nselected_slot={}\ndebug_hud={}\n",
            self.sensitivity,
            self.fov_degrees,
            self.view_distance,
            self.scale,
            self.fullscreen,
            self.selected_slot,
            self.debug_hud,
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
            "hotbar" => {
                if let Some(hotbar) = parse_hotbar(value) {
                    config.hotbar = hotbar;
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

fn parse_hotbar(value: &str) -> Option<[u8; 9]> {
    let parsed = value
        .split(',')
        .map(|slot| slot.trim().parse::<i64>().ok())
        .collect::<Vec<_>>();
    if parsed.len() != 9 {
        return None;
    }
    let defaults = Config::default().hotbar;
    Some(std::array::from_fn(|index| {
        parsed[index]
            .map(|block| block.clamp(1, i64::from(MAX_BLOCK)) as u8)
            .unwrap_or(defaults[index])
    }))
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
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("bloxgloom-{label}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn config_round_trips_through_explicit_path() {
        let directory = test_directory("config-roundtrip");
        let path = directory.join("settings/config");
        let config = Config {
            sensitivity: 0.006,
            fov_degrees: 92.5,
            view_distance: 5,
            scale: 1.25,
            fullscreen: true,
            hotbar: [3, 2, 1, 3, 2, 1, 3, 2, 1],
            selected_slot: 7,
            debug_hud: true,
        };

        config.save(&path).unwrap();

        assert_eq!(Config::load(&path), config);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_values_are_clamped_and_corrupt_files_fall_back() {
        let directory = test_directory("config-invalid");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings");
        fs::write(
            &path,
            "version=1\nsensitivity=NaN\nfov_degrees=500\nview_distance=255\nscale=-4\nfullscreen=true\nhotbar=0,2,99,1,2,3,1,2,3\nselected_slot=40\ndebug_hud=true\n",
        )
        .unwrap();

        let config = Config::load(&path);
        assert_eq!(config.sensitivity, Config::default().sensitivity);
        assert_eq!(config.fov_degrees, MAX_FOV);
        assert_eq!(config.view_distance, MAX_VIEW_DISTANCE);
        assert_eq!(config.scale, MIN_SCALE);
        assert_eq!(config.hotbar, [1, 2, 8, 1, 2, 3, 1, 2, 3]);
        assert_eq!(config.selected_slot, 8);
        assert!(config.fullscreen && config.debug_hud);

        fs::write(&path, "version=99\nfov_degrees=80\n").unwrap();
        assert_eq!(Config::load(&path), Config::default());
        assert_eq!(Config::load(directory.join("missing")), Config::default());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn saving_sanitizes_public_values() {
        let directory = test_directory("config-save-clamp");
        let path = directory.join("settings");
        let config = Config {
            sensitivity: f32::INFINITY,
            fov_degrees: -1.0,
            view_distance: 0,
            scale: 9.0,
            hotbar: [0, 4, 2, 1, 1, 1, 1, 1, 1],
            selected_slot: usize::MAX,
            ..Config::default()
        };

        config.save(&path).unwrap();
        let loaded = Config::load(&path);
        assert_eq!(loaded.sensitivity, Config::default().sensitivity);
        assert_eq!(loaded.fov_degrees, MIN_FOV);
        assert_eq!(loaded.view_distance, MIN_VIEW_DISTANCE);
        assert_eq!(loaded.scale, MAX_SCALE);
        assert_eq!(loaded.hotbar[0..2], [1, 4]);
        assert_eq!(loaded.selected_slot, 8);
        fs::remove_dir_all(directory).unwrap();
    }
}
