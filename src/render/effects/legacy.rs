//! Compatibility validator for the original fixed scene-color fragment.
use wgpu::naga;
pub(super) fn validate(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
    // Disallow large aggregate types before validation/backend lowering; a tiny
    // source declaration must not request a gigantic local array or struct.
    if module.types.iter().any(|(_, ty)| {
        !matches!(
            ty.inner,
            naga::TypeInner::Scalar(_)
                | naga::TypeInner::Vector { .. }
                | naga::TypeInner::Matrix { .. }
                | naga::TypeInner::Image { .. }
                | naga::TypeInner::Sampler { .. }
        )
    }) {
        return Err("only scalar/vector/matrix and fixed resource types are allowed".into());
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| e.to_string())?;
    if module.entry_points.len() != 1
        || !module.functions.is_empty()
        || module.global_variables.len() != 3
        || !module.overrides.is_empty()
        || module.types.len() > 64
        || module.global_expressions.len() > 128
    {
        return Err("shader resource/entrypoint limit exceeded".into());
    }
    let entry = &module.entry_points[0];
    let vec4 = |ty| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4
                }
            }
        )
    };
    if entry.name != "fs_main"
        || entry.stage != naga::ShaderStage::Fragment
        || entry.function.arguments.len() != 1
        || !matches!(
            entry.function.arguments[0].binding,
            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { .. }))
        )
        || !vec4(entry.function.arguments[0].ty)
        || entry.function.result.as_ref().is_none_or(|r| {
            !vec4(r.ty) || !matches!(r.binding, Some(naga::Binding::Location { location: 0, .. }))
        })
        || entry.function.expressions.len() > 512
        || entry.function.local_variables.len() > 32
    {
        return Err("expected only fs_main(@builtin(position) vec4f) -> @location(0) vec4f; expression limit 512".into());
    }
    let mut seen = [false; 3];
    for (_, global) in module.global_variables.iter() {
        let Some(binding) = global.binding.as_ref() else {
            return Err("only fixed bound resources allowed".into());
        };
        if binding.group != 0 || binding.binding > 2 || seen[binding.binding as usize] {
            return Err("binding allowlist: group 0 bindings 0,1,2 exactly once".into());
        }
        seen[binding.binding as usize] = true;
        let valid = match binding.binding {
            0 => {
                global.space == naga::AddressSpace::Handle
                    && matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Image {
                            dim: naga::ImageDimension::D2,
                            arrayed: false,
                            class: naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Float,
                                multi: false
                            }
                        }
                    )
            }
            1 => {
                global.space == naga::AddressSpace::Handle
                    && matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: false }
                    )
            }
            2 => global.space == naga::AddressSpace::Uniform && vec4(global.ty),
            _ => false,
        };
        if !valid {
            return Err(
                "binding type mismatch (scene texture, filtering sampler, vec4f time/size)".into(),
            );
        }
    }
    // Straight-line fragments only: no loops, nested control flow, calls,
    // discard, storage writes or atomics. Math and texture sampling remain useful.
    if entry.function.body.len() > 128
        || entry.function.body.iter().any(|s| {
            !matches!(
                s,
                naga::Statement::Emit(_)
                    | naga::Statement::Return { .. }
                    | naga::Statement::Store { .. }
            )
        })
    {
        return Err("only straight-line fragment math is allowed (128 statement limit)".into());
    }
    Ok(())
}
