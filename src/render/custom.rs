//! Verified package materials with stable surface hooks and bounded parameters.
use super::parameters::{self, Definition};
use crate::{content::Catalog, server::client_bundle::ClientPackage};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

mod gpu;
mod legacy;
mod shader;
pub(crate) use gpu::Gpu;
pub(super) use shader::TYPES;
#[cfg(test)]
mod tests;

pub(crate) const MAX_DESCRIPTOR_BYTES: usize = 4096;
pub(crate) const MAX_SHADER_BYTES: usize = 8 * 1024;
pub(crate) const MAX_MATERIALS: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    #[serde(default = "legacy_version")]
    version: u8,
    shader: String,
    texture: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    textures: Vec<String>,
    #[serde(default)]
    parameters: Vec<Definition>,
    #[serde(default)]
    vertex_offset: f32,
    #[serde(default, deserialize_with = "deserialize_optional_object")]
    environment_lighting: Option<crate::config::lighting::Lighting>,
    #[serde(default, deserialize_with = "deserialize_optional_object")]
    local_shadows: Option<super::local_shadow::Settings>,
}
// A JSON object is required: derived struct deserializers otherwise also accept
// positional arrays, which are not part of the authored asset contract.
fn deserialize_optional_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Selection<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Selection<T> {
        type Value = Option<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a settings object or null")
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Some)
        }
    }
    deserializer.deserialize_any(Selection(std::marker::PhantomData))
}

fn legacy_version() -> u8 {
    1
}

#[derive(Debug)]
pub(crate) struct MaterialSource {
    pub owner: String,
    targets: Vec<String>,
    textures: Vec<String>,
    pub parameters: Vec<Definition>,
    shader: String,
    version: u8,
    vertex_offset: f32,
}
#[derive(Debug)]
pub(crate) struct Source {
    pub materials: Vec<MaterialSource>,
    /// One explicit package-owned global lighting selection for this session.
    pub environment_lighting: Option<crate::config::lighting::Lighting>,
    pub local_shadows: Option<super::local_shadow::Settings>,
}

pub(crate) fn prepare_assets(
    packages: &BTreeMap<String, ClientPackage>,
) -> Result<Option<Source>, String> {
    if packages.values().all(|p| p.material_assets.is_empty()) {
        return Ok(None);
    }
    std::thread::scope(|scope| {
        scope
            .spawn(|| prepare_assets_inner(packages))
            .join()
            .map_err(|_| "material preparation worker panicked".to_string())?
    })
}
fn prepare_assets_inner(
    packages: &BTreeMap<String, ClientPackage>,
) -> Result<Option<Source>, String> {
    let mut materials = Vec::new();
    let mut environment_lighting = None;
    let mut local_shadows = None;
    let mut local_shadow_owner: Option<String> = None;
    let mut environment_owner: Option<String> = None;
    let mut targets = BTreeSet::new();
    for (package, data) in packages {
        let mut used = BTreeSet::new();
        for (name, (_, bytes)) in data
            .material_assets
            .iter()
            .filter(|(_, (kind, _))| *kind == 8)
        {
            let owner = format!("{package}:{name}");
            let fail = |e: String| format!("{owner}: material: {e}");
            if materials.len() == MAX_MATERIALS || bytes.len() > MAX_DESCRIPTOR_BYTES {
                return Err(fail("material count or descriptor limit exceeded".into()));
            }
            let mut descriptor: Descriptor =
                serde_json::from_slice(bytes).map_err(|e| fail(e.to_string()))?;
            if !parameters::identifier(&descriptor.shader) {
                return Err(fail("shader must be a local asset key".into()));
            }
            let (kind, bytes) = data
                .material_assets
                .get(&descriptor.shader)
                .ok_or_else(|| fail("missing material shader".into()))?;
            if *kind != 9 || bytes.len() > MAX_SHADER_BYTES {
                return Err(fail("invalid material shader kind or size".into()));
            }
            used.insert(descriptor.shader.clone());
            let source = std::str::from_utf8(bytes).map_err(|e| fail(e.to_string()))?;
            match descriptor.version {
                1 => {
                    if bytes.len() > 1024
                        || !descriptor.targets.is_empty()
                        || !descriptor.textures.is_empty()
                        || !descriptor.parameters.is_empty()
                        || descriptor.vertex_offset != 0.0
                        || descriptor.environment_lighting.is_some()
                        || descriptor.local_shadows.is_some()
                    {
                        return Err(fail("extended material fields require version 2".into()));
                    }
                    let texture = descriptor
                        .texture
                        .take()
                        .ok_or_else(|| fail("missing texture".into()))?;
                    descriptor.targets.push(texture.clone());
                    descriptor.textures.push(texture);
                    legacy::validate(source).map_err(fail)?;
                }
                2 => {
                    if descriptor.texture.is_some()
                        || descriptor.targets.is_empty()
                        || descriptor.targets.len() > 8
                        || descriptor.textures.is_empty()
                        || descriptor.textures.len() > 4
                        || !descriptor.vertex_offset.is_finite()
                        || !(0.0..=0.25).contains(&descriptor.vertex_offset)
                    {
                        return Err(fail("invalid version-2 material targets, texture inputs or vertex offset (0..0.25)".into()));
                    }
                    if let Some(lighting) = descriptor.environment_lighting {
                        lighting.validate().map_err(fail)?;
                        if let Some(previous) = &environment_owner {
                            return Err(fail(format!(
                                "environment lighting already selected by {previous}"
                            )));
                        }
                        environment_owner = Some(owner.clone());
                        environment_lighting = Some(lighting);
                    }
                    if let Some(settings) = descriptor.local_shadows {
                        settings.validate().map_err(fail)?;
                        if let Some(previous) = &local_shadow_owner {
                            return Err(fail(format!(
                                "local shadows already selected by {previous}"
                            )));
                        }
                        local_shadow_owner = Some(owner.clone());
                        local_shadows = Some(settings);
                    }
                    parameters::defaults(&descriptor.parameters).map_err(fail)?;
                    shader::validate(source).map_err(fail)?;
                }
                _ => return Err(fail("unsupported material contract version".into())),
            }
            for key in descriptor.targets.iter().chain(&descriptor.textures) {
                let valid = key.split_once(':').is_some_and(|(owner, name)| {
                    (owner == package || owner == "bloxgloom") && parameters::identifier(name)
                });
                if !valid {
                    return Err(fail(format!(
                        "texture {key} must belong to this package or bloxgloom"
                    )));
                }
            }
            for target in &descriptor.targets {
                if !targets.insert(target.clone()) {
                    return Err(fail(format!("duplicate material ownership of {target}")));
                }
            }
            materials.push(MaterialSource {
                owner,
                targets: descriptor.targets,
                textures: descriptor.textures,
                parameters: descriptor.parameters,
                shader: source.into(),
                version: descriptor.version,
                vertex_offset: descriptor.vertex_offset,
            });
        }
        if data
            .material_assets
            .iter()
            .any(|(key, (kind, _))| *kind == 9 && !used.contains(key))
        {
            return Err(format!("{package}: orphan material shader"));
        }
    }
    Ok(Some(Source {
        materials,
        environment_lighting,
        local_shadows,
    }))
}
impl Source {
    pub(crate) fn resolve(&self, catalog: &Catalog) -> Result<Prepared, String> {
        let mut materials = Vec::new();
        for source in &self.materials {
            let resolve = |key: &String| {
                catalog
                    .textures()
                    .iter()
                    .position(|texture| texture.key == *key)
                    .map(|layer| layer as u32)
                    .ok_or_else(|| {
                        format!(
                            "{}: texture {key} is not registered in the session catalog",
                            source.owner
                        )
                    })
            };
            materials.push(Material {
                owner: source.owner.clone(),
                layers: source
                    .targets
                    .iter()
                    .map(resolve)
                    .collect::<Result<_, _>>()?,
                textures: source
                    .textures
                    .iter()
                    .map(resolve)
                    .collect::<Result<_, _>>()?,
                parameters: source.parameters.clone(),
                shader: source.shader.clone(),
                version: source.version,
                vertex_offset: source.vertex_offset,
            });
        }
        Ok(Prepared {
            materials,
            environment_lighting: self.environment_lighting,
            local_shadows: self.local_shadows,
        })
    }
}
#[derive(Debug, Clone)]
pub(crate) struct Material {
    pub owner: String,
    pub layers: Vec<u32>,
    pub textures: Vec<u32>,
    pub parameters: Vec<Definition>,
    shader: String,
    version: u8,
    vertex_offset: f32,
}
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub materials: Vec<Material>,
    pub environment_lighting: Option<crate::config::lighting::Lighting>,
    pub local_shadows: Option<super::local_shadow::Settings>,
}
#[cfg(test)]
impl Prepared {
    pub(crate) fn selected_layer(&self) -> u32 {
        self.materials[0].layers[0]
    }
}
#[cfg(test)]
pub(crate) fn prepare(
    owner: &str,
    source: &[u8],
    layer: u32,
    layer_count: u32,
) -> Result<Prepared, String> {
    if source.len() > MAX_SHADER_BYTES || layer >= layer_count || layer_count == 0 {
        return Err(format!(
            "{owner}: material resource or texture layer limit exceeded"
        ));
    }
    let source = std::str::from_utf8(source).map_err(|e| format!("{owner}: {e}"))?;
    legacy::validate(source).map_err(|e| format!("{owner}: {e}"))?;
    Ok(Prepared {
        environment_lighting: None,
        local_shadows: None,
        materials: vec![Material {
            owner: owner.into(),
            layers: vec![layer],
            textures: vec![layer],
            parameters: vec![],
            shader: source.into(),
            version: 1,
            vertex_offset: 0.0,
        }],
    })
}
pub(super) fn compose(prepared: &Prepared) -> String {
    shader::compose(prepared)
}
