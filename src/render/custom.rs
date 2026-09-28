//! Bounded fragment-only voxel material customization. Geometry, UVs, camera,
//! light, emission, alpha cutout and fog remain renderer-owned. Authored code
//! changes only albedo before the standard light/emission/fog calculation.
use std::collections::BTreeMap;

use serde::Deserialize;
use wgpu::naga;

use crate::content::Catalog;
use crate::server::client_bundle::ClientPackage;

#[cfg(test)]
mod tests;

pub(crate) const MAX_SHADER_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    shader: String,
    texture: String,
}

#[derive(Debug)]
pub(crate) struct Source {
    pub owner: String,
    texture: String,
    shader: String,
}

/// Prepare exactly one locally owned descriptor and shader across the whole
/// verified bundle. Independent UI and scene-color effect assets are unaffected.
pub(crate) fn prepare_assets(
    packages: &BTreeMap<String, ClientPackage>,
) -> Result<Option<Source>, String> {
    let Some(owner) = packages
        .iter()
        .find(|(_, package)| !package.material_assets.is_empty())
        .map(|(name, _)| name)
    else {
        return Ok(None);
    };
    std::thread::scope(|scope| {
        scope
            .spawn(|| prepare_assets_inner(packages))
            .join()
            .map_err(|_| format!("{owner}: material preparation worker panicked"))?
    })
}

fn prepare_assets_inner(
    packages: &BTreeMap<String, ClientPackage>,
) -> Result<Option<Source>, String> {
    let mut result = None;
    for (package, data) in packages {
        if data.material_assets.is_empty() {
            continue;
        }
        let fail = |message: &str| format!("{package}: material: {message}");
        if data.material_assets.len() != 2 || result.is_some() {
            return Err(fail(
                "duplicate ownership or orphan material assets (one material per bundle)",
            ));
        }
        let (name, (_, bytes)) = data
            .material_assets
            .iter()
            .find(|(_, (kind, _))| *kind == 8)
            .ok_or_else(|| fail("missing material descriptor"))?;
        if bytes.len() > 1024 {
            return Err(fail("descriptor exceeds 1024 bytes"));
        }
        let descriptor: Descriptor =
            serde_json::from_slice(bytes).map_err(|e| fail(&format!("{name}: {e}")))?;
        let local = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        };
        let (texture_owner, texture_name) = descriptor
            .texture
            .split_once(':')
            .ok_or_else(|| fail("texture must be a namespaced catalog key"))?;
        if !local(&descriptor.shader)
            || !local(texture_owner)
            || !local(texture_name)
            || (texture_owner != package && texture_owner != "bloxgloom")
        {
            return Err(fail(
                "shader must be local and texture owned by the package or bloxgloom",
            ));
        }
        let (kind, shader) = data
            .material_assets
            .get(&descriptor.shader)
            .ok_or_else(|| fail("missing local material shader"))?;
        if *kind != 9 || shader.len() > MAX_SHADER_BYTES {
            return Err(fail("material shader kind or 8 KiB resource limit"));
        }
        let owner = format!("{package}:{name}");
        let shader = std::str::from_utf8(shader).map_err(|e| format!("{owner}: {e}"))?;
        validate(shader).map_err(|e| format!("{owner}: shader {}: {e}", descriptor.shader))?;
        result = Some(Source {
            owner,
            texture: descriptor.texture,
            shader: shader.into(),
        });
    }
    Ok(result)
}

impl Source {
    /// Resolve only after the connection catalog has matched the server's
    /// authoritative content manifest. Never persist the transient layer ID.
    pub(crate) fn resolve(&self, catalog: &Catalog) -> Result<Prepared, String> {
        let layer = catalog
            .textures()
            .iter()
            .position(|texture| texture.key == self.texture)
            .ok_or_else(|| {
                format!(
                    "{}: texture {} is not registered in the session catalog",
                    self.owner, self.texture
                )
            })?;
        Ok(Prepared {
            owner: self.owner.clone(),
            source: self.shader.clone(),
            layer: layer as u32,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Prepared {
    pub owner: String,
    source: String,
    layer: u32,
}

/// Call from asset preparation, not the window thread. The layer is a catalog
/// texture ID, not a block ID: face-specific art and item sprites share it.
#[cfg(test)]
pub(crate) fn prepare(
    owner: &str,
    source: &[u8],
    layer: u32,
    layer_count: u32,
) -> Result<Prepared, String> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| prepare_inner(owner, source, layer, layer_count))
            .join()
            .map_err(|_| format!("{owner}: material shader preparation worker panicked"))?
    })
}

#[cfg(test)]
fn prepare_inner(
    owner: &str,
    source: &[u8],
    layer: u32,
    layer_count: u32,
) -> Result<Prepared, String> {
    let fail = |message: String| format!("{owner}: material shader: {message}");
    if source.len() > MAX_SHADER_BYTES {
        return Err(fail("8 KiB resource limit exceeded".into()));
    }
    if layer >= layer_count || layer_count == 0 {
        return Err(fail(format!(
            "texture layer {layer} outside catalog (0..{layer_count})"
        )));
    }
    let source = std::str::from_utf8(source).map_err(|e| fail(e.to_string()))?;
    validate(source).map_err(fail)?;
    Ok(Prepared {
        owner: owner.into(),
        source: source.into(),
        layer,
    })
}

fn validate(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
    let vec = |ty, size| {
        matches!(module.types[ty].inner,
        naga::TypeInner::Vector { size: actual, scalar: naga::Scalar { kind: naga::ScalarKind::Float, width: 4 } } if actual == size)
    };
    if !module.entry_points.is_empty()
        || module.functions.len() != 1
        || !module.global_variables.is_empty()
        || !module.constants.is_empty()
        || !module.overrides.is_empty()
        || !module.global_expressions.is_empty()
        || module.types.len() > 24
        || module.types.iter().any(|(_, ty)| {
            !matches!(
                ty.inner,
                naga::TypeInner::Scalar(_)
                    | naga::TypeInner::Vector { .. }
                    | naga::TypeInner::Matrix { .. }
            )
        })
    {
        return Err(
            "expected one pure custom_albedo function and no resources, globals or entry points"
                .into(),
        );
    }
    let (_, function) = module.functions.iter().next().unwrap();
    if function.name.as_deref() != Some("custom_albedo")
        || function.arguments.len() != 3
        || !vec(function.arguments[0].ty, naga::VectorSize::Tri)
        || !vec(function.arguments[1].ty, naga::VectorSize::Bi)
        || !vec(function.arguments[2].ty, naga::VectorSize::Tri)
        || function.arguments.iter().any(|arg| arg.binding.is_some())
        || function
            .result
            .as_ref()
            .is_none_or(|result| result.binding.is_some() || !vec(result.ty, naga::VectorSize::Tri))
    {
        return Err(
            "expected fn custom_albedo(albedo: vec3f, uv: vec2f, world_position: vec3f) -> vec3f"
                .into(),
        );
    }
    if function.expressions.len() > 256
        || function.local_variables.len() > 16
        || function.body.len() > 96
        || function.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_)
                    | naga::Statement::Store { .. }
                    | naga::Statement::Return { .. }
            )
        })
        || function.expressions.iter().any(|(_, expression)| {
            !matches!(
                expression,
                naga::Expression::Literal(_)
                    | naga::Expression::ZeroValue(_)
                    | naga::Expression::Compose { .. }
                    | naga::Expression::Access { .. }
                    | naga::Expression::AccessIndex { .. }
                    | naga::Expression::Splat { .. }
                    | naga::Expression::Swizzle { .. }
                    | naga::Expression::FunctionArgument(_)
                    | naga::Expression::LocalVariable(_)
                    | naga::Expression::Load { .. }
                    | naga::Expression::Unary { .. }
                    | naga::Expression::Binary { .. }
                    | naga::Expression::Select { .. }
                    | naga::Expression::Relational { .. }
                    | naga::Expression::Math { .. }
                    | naga::Expression::As { .. }
            )
        })
    {
        return Err(
            "only bounded straight-line arithmetic is allowed (256 expressions, 96 statements)"
                .into(),
        );
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn compose(builtin: &str, prepared: &Prepared) -> String {
    // Only this selected tile uses the custom function; builtins and all other
    // catalog layers follow the byte-for-byte old shading expression.
    let custom = format!(
        "select(albedo, custom_albedo(albedo, input.uv, input.world_position), u32(input.layer) == {}u)",
        prepared.layer
    );
    let shader = builtin
        .replace("let emission = albedo * material_emission[u32(input.layer)];", &format!("let shaded_albedo = {custom};\n    let emission = shaded_albedo * material_emission[u32(input.layer)];"))
        .replace("albedo * input.light + emission", "shaded_albedo * input.light + emission")
        .replace(
            "@location(4) sky_level: f32,",
            "@location(4) sky_level: f32,\n    @location(5) world_position: vec3<f32>,",
        )
        .replace(
            "output.sky_level = sky;",
            "output.sky_level = sky;\n    output.world_position = input.position;",
        );
    format!("{shader}\n{}", prepared.source)
}
