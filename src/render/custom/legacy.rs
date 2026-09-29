use wgpu::naga;

pub(super) fn validate(source: &str) -> Result<(), String> {
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
