//! Shared finite-work checks and identifier isolation for versioned WGSL hooks.
use wgpu::naga;

pub(super) fn validate(module: &naga::Module) -> Result<(), String> {
    if !module.entry_points.is_empty()
        || !module.global_variables.is_empty()
        || !module.constants.is_empty()
        || !module.overrides.is_empty()
        || module.functions.len() > 12
        || module.types.len() > 64
        || module.types.iter().any(|(_, ty)| {
            !matches!(
                ty.inner,
                naga::TypeInner::Scalar(_)
                    | naga::TypeInner::Vector { .. }
                    | naga::TypeInner::Matrix { .. }
                    | naga::TypeInner::Struct { span: 0..=1024, .. }
            )
        })
    {
        return Err("hooks allow bounded functions and structs, without resource declarations or entry points".into());
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(module)
    .map_err(|e| e.to_string())?;
    fn cost(block: &naga::Block, costs: &[usize]) -> Option<usize> {
        let mut total = 0usize;
        for statement in block.iter() {
            total = total.checked_add(
                1 + match statement {
                    naga::Statement::Emit(_)
                    | naga::Statement::Store { .. }
                    | naga::Statement::Return { .. } => 0,
                    naga::Statement::Call { function, .. } => *costs.get(function.index())?,
                    naga::Statement::Block(block) => cost(block, costs)?,
                    naga::Statement::If { accept, reject, .. } => {
                        cost(accept, costs)? + cost(reject, costs)?
                    }
                    _ => return None,
                },
            )?;
            if total > 4096 {
                return None;
            }
        }
        Some(total)
    }
    let mut costs = Vec::new();
    for (_, function) in module.functions.iter() {
        let work =
            cost(&function.body, &costs).and_then(|n| n.checked_add(function.expressions.len()));
        if function.local_variables.len() > 32
            || function.expressions.len() > 512
            || work.is_none_or(|n| n > 4096)
        {
            return Err("hook exceeds finite work limits or uses unsupported control flow".into());
        }
        costs.push(work.unwrap());
    }
    Ok(())
}

// WGSL has no string literals. Renaming identifiers also in comments is harmless.
pub(super) fn rename(
    source: &str,
    names: &std::collections::BTreeSet<String>,
    prefix: &str,
) -> String {
    let mut result = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, result: &mut String| {
        if names.contains(token) {
            result.push_str(prefix);
        }
        result.push_str(token);
        token.clear();
    };
    for character in source.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            token.push(character);
        } else {
            flush(&mut token, &mut result);
            result.push(character);
        }
    }
    flush(&mut token, &mut result);
    result
}
