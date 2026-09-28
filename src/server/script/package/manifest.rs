use std::collections::{BTreeMap, BTreeSet};

use super::error;
use crate::server::script::ScriptError;

pub(super) struct Manifest {
    pub version: String,
    pub entry: String,
    pub dependencies: BTreeMap<String, String>,
    pub modules: BTreeMap<String, String>,
    pub requires: BTreeSet<String>,
}

impl Manifest {
    pub fn parse(directory: &str, text: &str) -> Result<Self, ScriptError> {
        let fail = || {
            error(
                directory,
                "invalid package.txt (format, identity, version, entry, dependency or module declaration)",
            )
        };
        let mut format = false;
        let mut name = None;
        let mut version = None;
        let mut entry = None;
        let mut dependencies = BTreeMap::new();
        let mut modules = BTreeMap::new();
        let mut requires = BTreeSet::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            // Four tokens suffice to reject malformed lines without allocating
            // a token vector proportional to input whitespace.
            let mut words = line.split_ascii_whitespace();
            let fields = (words.next(), words.next(), words.next(), words.next());
            match fields {
                (Some("format"), Some("1"), None, None) if !format => format = true,
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
                    if identifier(key) && valid_path(value) && modules.len() < 64 =>
                {
                    if modules.insert(key.to_owned(), value.to_owned()).is_some() {
                        return Err(fail());
                    }
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
        if !format || name.is_none() || !modules.contains_key(&entry) {
            return Err(fail());
        }
        Ok(Self {
            version: version.ok_or_else(fail)?,
            entry,
            dependencies,
            modules,
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

fn valid_version(value: &str) -> bool {
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
    path.len() <= 240
        && path.ends_with(".luau")
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
