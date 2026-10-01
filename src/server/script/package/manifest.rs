use std::collections::{BTreeMap, BTreeSet};

use super::error;
use crate::server::script::ScriptError;
use crate::server::script::capacity::{MAX_ASSETS_PER_PACKAGE, MAX_MODULES_PER_PACKAGE};

pub(super) struct Manifest {
    pub version: String,
    pub entry: String,
    pub dependencies: BTreeMap<String, String>,
    pub modules: BTreeMap<String, String>,
    pub sides: BTreeMap<String, SourceSide>,
    pub assets: BTreeMap<String, String>,
    pub asset_kinds: BTreeMap<String, u32>,
    pub requires: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSide {
    Server,
    Client,
    Shared,
}

impl Manifest {
    pub fn parse(directory: &str, text: &str) -> Result<Self, ScriptError> {
        let fail = || {
            error(
                directory,
                "invalid package.txt (format, identity, version, entry, dependency, module or asset declaration)",
            )
        };
        let mut format = None;
        let mut name = None;
        let mut version = None;
        let mut entry = None;
        let mut dependencies = BTreeMap::new();
        let mut modules = BTreeMap::new();
        let mut sides = BTreeMap::new();
        let mut assets = BTreeMap::new();
        let mut asset_kinds = BTreeMap::new();
        let mut legacy_modules = false;
        let mut classified_files = false;
        let mut requires = BTreeSet::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            // Five tokens suffice to reject malformed lines without allocating
            // a token vector proportional to input whitespace.
            let mut words = line.split_ascii_whitespace();
            let fields = (words.next(), words.next(), words.next(), words.next());
            if words.next().is_some() {
                return Err(fail());
            }
            match fields {
                (Some("format"), Some(value @ ("1" | "2")), None, None) if format.is_none() => {
                    format = Some(value);
                }
                (Some("package"), Some(value), None, None)
                    if name.is_none() && value == directory =>
                {
                    name = Some(value)
                }
                (Some("version"), Some(value), None, None)
                    if version.is_none() && valid_version(value) =>
                {
                    version = Some(value.to_owned())
                }
                (Some("entry"), Some(value), None, None)
                    if entry.is_none() && identifier(value) =>
                {
                    entry = Some(value.to_owned())
                }
                (Some("dependency"), Some(key), Some(value), None)
                    if identifier(key)
                        && key != directory
                        && valid_version(value)
                        && dependencies.len() < 32 =>
                {
                    if dependencies
                        .insert(key.to_owned(), value.to_owned())
                        .is_some()
                    {
                        return Err(fail());
                    }
                }
                (Some("module"), Some(key), Some(value), None)
                    if identifier(key) && valid_path(value) =>
                {
                    if modules.len() == MAX_MODULES_PER_PACKAGE {
                        return Err(error(
                            directory,
                            format!(
                                "module {key}: modules/package: attempted {}; maximum {MAX_MODULES_PER_PACKAGE}",
                                modules.len() + 1
                            ),
                        ));
                    }
                    if modules.insert(key.to_owned(), value.to_owned()).is_some() {
                        return Err(fail());
                    }
                    sides.insert(key.to_owned(), SourceSide::Server);
                    legacy_modules = true;
                }
                (Some("module"), Some(side), Some(key), Some(path))
                    if identifier(key) && valid_path(path) =>
                {
                    if modules.len() == MAX_MODULES_PER_PACKAGE {
                        return Err(error(
                            directory,
                            format!(
                                "module {key}: modules/package: attempted {}; maximum {MAX_MODULES_PER_PACKAGE}",
                                modules.len() + 1
                            ),
                        ));
                    }
                    let kind = match side {
                        "server" => SourceSide::Server,
                        "client" => SourceSide::Client,
                        "shared" => SourceSide::Shared,
                        _ => return Err(fail()),
                    };
                    if !path.starts_with(&format!("{side}/"))
                        || !public_path(path)
                        || modules.insert(key.to_owned(), path.to_owned()).is_some()
                    {
                        return Err(fail());
                    }
                    sides.insert(key.to_owned(), kind);
                    classified_files = true;
                }
                (Some("asset"), Some(kind), Some(key), Some(path))
                    if identifier(key) && asset_path(kind, path).is_some() =>
                {
                    if assets.len() == MAX_ASSETS_PER_PACKAGE {
                        return Err(error(
                            directory,
                            format!(
                                "asset {key} {path}: assets/package: attempted {}; maximum {MAX_ASSETS_PER_PACKAGE}",
                                assets.len() + 1
                            ),
                        ));
                    }
                    if assets.insert(key.to_owned(), path.to_owned()).is_some() {
                        return Err(fail());
                    }
                    asset_kinds.insert(key.to_owned(), asset_path(kind, path).unwrap());
                    classified_files = true;
                }
                (Some("requires"), Some(value), None, None)
                    if value.len() <= 255 && requires.len() < 32 =>
                {
                    if !requires.insert(value.to_owned()) {
                        return Err(fail());
                    }
                }
                _ => return Err(fail()),
            }
        }
        let entry = entry.ok_or_else(fail)?;
        if format.is_none()
            || name.is_none()
            || !modules.contains_key(&entry)
            || sides.get(&entry) == Some(&SourceSide::Client)
            || (format == Some("1") && classified_files)
            || (format == Some("2") && legacy_modules)
        {
            return Err(fail());
        }
        Ok(Self {
            version: version.ok_or_else(fail)?,
            entry,
            dependencies,
            modules,
            sides,
            assets,
            asset_kinds,
            requires,
        })
    }
}

pub(in crate::server::script) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub(super) fn valid_version(value: &str) -> bool {
    let mut parts = value.split('.');
    (0..3).all(|_| {
        parts.next().is_some_and(|p| {
            !p.is_empty()
                && (p == "0" || !p.starts_with('0'))
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u32>().is_ok()
        })
    }) && parts.next().is_none()
}

fn valid_path(path: &str) -> bool {
    path.ends_with(".luau") && bounded_path(path)
}

fn bounded_path(path: &str) -> bool {
    path.len() <= 240
        && path.split('/').count() <= 8
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}

// Exportable files live in dedicated trees, never save paths or dotfiles.
fn public_path(path: &str) -> bool {
    bounded_path(path) && path.split('/').all(|part| !part.starts_with('.'))
}

fn asset_path(kind: &str, path: &str) -> Option<u32> {
    let (tag, directory, suffix) = match kind {
        "texture" => (1, "assets/textures/", ".png"),
        "ui-document" => (2, "assets/ui/", ".json"),
        "ui-style" => (3, "assets/ui/", ".json"),
        "ui-font" => (4, "assets/fonts/", ".ttf"),
        "ui-image" => (5, "assets/ui/", ".png"),
        "shader" => (6, "assets/shaders/", ".wgsl"),
        "effect" => (7, "assets/effects/", ".json"),
        "material" => (8, "assets/materials/", ".json"),
        "material-shader" => (9, "assets/shaders/", ".wgsl"),
        _ => return None,
    };
    (public_path(path) && path.starts_with(directory) && path.ends_with(suffix)).then_some(tag)
}
