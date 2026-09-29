//! Declarative, finite scene-color graphs. Packages never submit GPU commands.
use super::parameters::{self, Definition};
use crate::server::client_bundle::ClientPackage;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
mod gpu;
mod graph;
mod legacy;
mod shader;
pub(super) use gpu::Effect;
#[cfg(test)]
use legacy::validate;
#[cfg(test)]
mod tests;

pub(crate) const MAX_DESCRIPTOR_BYTES: usize = 4096;
pub(crate) const MAX_SHADER_BYTES: usize = 16 * 1024;
pub(crate) const MAX_PASSES: usize = 8;
pub(crate) const SCENE: &str = "bloxgloom:scene_color";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    #[serde(default = "version_one")]
    version: u8,
    shader: String,
    stage: Option<Stage>,
    #[serde(default)]
    order: u8,
    #[serde(default)]
    inputs: Vec<String>,
    output: Option<String>,
    #[serde(default)]
    after: Vec<String>,
    #[serde(default, rename = "final")]
    final_output: bool,
    #[serde(default = "version_one")]
    scale: u8,
    #[serde(default)]
    parameters: Vec<Definition>,
}
fn version_one() -> u8 {
    1
}
#[derive(Debug, Clone, Deserialize)]
enum Stage {
    #[serde(rename = "scene_color")]
    SceneColor,
}
#[derive(Debug, Clone)]
pub(crate) struct Pass {
    pub owner: String,
    descriptor: Descriptor,
    source: String,
}
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    #[cfg(test)]
    pub owner: String,
    pub passes: Vec<Pass>,
    final_index: usize,
}
impl Pass {
    pub(crate) fn parameters(&self) -> &[Definition] {
        &self.descriptor.parameters
    }
}

pub(crate) fn prepare(
    packages: &BTreeMap<String, ClientPackage>,
) -> Result<Option<Prepared>, String> {
    if packages.values().all(|p| p.effect_assets.is_empty()) {
        return Ok(None);
    }
    std::thread::scope(|scope| {
        scope
            .spawn(|| prepare_inner(packages))
            .join()
            .map_err(|_| "effect preparation worker panicked".to_owned())?
    })
}
fn prepare_inner(packages: &BTreeMap<String, ClientPackage>) -> Result<Option<Prepared>, String> {
    let mut passes = Vec::new();
    for (package, data) in packages {
        let mut used = BTreeSet::new();
        for (name, (_, bytes)) in data
            .effect_assets
            .iter()
            .filter(|(_, (kind, _))| *kind == 7)
        {
            let owner = format!("{package}:{name}");
            let fail = |e: String| format!("{owner}: effect: {e}");
            if passes.len() == MAX_PASSES || bytes.len() > MAX_DESCRIPTOR_BYTES {
                return Err(fail("effect count or descriptor limit exceeded".into()));
            }
            let mut descriptor: Descriptor =
                serde_json::from_slice(bytes).map_err(|e| fail(e.to_string()))?;
            if !parameters::identifier(&descriptor.shader) {
                return Err(fail("shader must be a local asset key".into()));
            }
            let (kind, source) = data
                .effect_assets
                .get(&descriptor.shader)
                .ok_or_else(|| fail("missing local shader".into()))?;
            if *kind != 6 || source.len() > MAX_SHADER_BYTES {
                return Err(fail("shader kind or 16 KiB resource limit".into()));
            }
            let source =
                std::str::from_utf8(source).map_err(|_| fail("shader is not UTF-8".into()))?;
            used.insert(descriptor.shader.clone());
            match descriptor.version {
                1 => {
                    if bytes.len() > 1024
                        || descriptor.stage.is_none()
                        || descriptor.order != 0
                        || !descriptor.inputs.is_empty()
                        || descriptor.output.is_some()
                        || !descriptor.after.is_empty()
                        || descriptor.final_output
                        || descriptor.scale != 1
                        || !descriptor.parameters.is_empty()
                    {
                        return Err(fail(
                            "legacy scene_color descriptor requires stage and order 0 only".into(),
                        ));
                    }
                    legacy::validate(source).map_err(fail)?;
                    descriptor.inputs.push(SCENE.into());
                    descriptor.output = Some(owner.clone());
                    descriptor.final_output = true;
                }
                2 => {
                    if descriptor.stage.is_some()
                        || descriptor.inputs.is_empty()
                        || descriptor.inputs.len() > 2
                        || descriptor.output.is_none()
                        || descriptor.after.len() > MAX_PASSES
                        || !matches!(descriptor.scale, 1 | 2 | 4)
                    {
                        return Err(fail(
                            "version 2 requires one/two inputs, an output, and scale 1, 2 or 4"
                                .into(),
                        ));
                    }
                    let output = descriptor.output.as_ref().unwrap();
                    if output
                        .split_once(':')
                        .is_none_or(|(p, local)| p != package || !parameters::identifier(local))
                    {
                        return Err(fail("output must belong to this package".into()));
                    }
                    for resource in descriptor.inputs.iter().chain(&descriptor.after) {
                        if resource == SCENE && descriptor.inputs.contains(resource) {
                            continue;
                        }
                        if resource.split_once(':').is_none_or(|(p, local)| {
                            !parameters::identifier(local)
                                || (p != package && !data.dependencies.contains_key(p))
                        }) {
                            return Err(fail(format!(
                                "{resource}: resource must belong to this package or an exact direct dependency"
                            )));
                        }
                    }
                    parameters::defaults(&descriptor.parameters).map_err(fail)?;
                    shader::validate(source).map_err(fail)?;
                }
                _ => return Err(fail("unsupported effect contract version".into())),
            }
            passes.push(Pass {
                owner,
                descriptor,
                source: source.into(),
            });
        }
        if data
            .effect_assets
            .iter()
            .any(|(key, (kind, _))| *kind == 6 && !used.contains(key))
        {
            return Err(format!("{package}: orphan effect shader"));
        }
    }
    graph::prepare(passes).map(Some)
}
