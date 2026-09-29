//! Deterministic dependency scheduling and compatibility diagnostics.
use super::{Pass, Prepared, SCENE};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

pub(super) fn prepare(passes: Vec<Pass>) -> Result<Prepared, String> {
    let mut outputs = BTreeMap::new();
    let owners = passes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.owner.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    for (index, pass) in passes.iter().enumerate() {
        let output = pass.descriptor.output.as_ref().unwrap();
        if output == SCENE || outputs.insert(output.as_str(), index).is_some() {
            return Err(format!(
                "{}: duplicate or reserved effect output {output}",
                pass.owner
            ));
        }
    }
    if passes.iter().filter(|p| p.descriptor.final_output).count() != 1 {
        return Err(format!(
            "{}: effect graph requires exactly one final output",
            passes
                .iter()
                .map(|p| p.owner.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut dependencies = Vec::new();
    for pass in &passes {
        let mut required = BTreeSet::new();
        for input in &pass.descriptor.inputs {
            if input != SCENE {
                required.insert(
                    *outputs
                        .get(input.as_str())
                        .ok_or_else(|| format!("{}: missing effect input {input}", pass.owner))?,
                );
            }
        }
        for after in &pass.descriptor.after {
            required.insert(
                *owners
                    .get(after.as_str())
                    .ok_or_else(|| format!("{}: missing effect dependency {after}", pass.owner))?,
            );
        }
        dependencies.push(required);
    }
    let final_original = passes
        .iter()
        .position(|p| p.descriptor.final_output)
        .unwrap();
    // Every allocated pass must contribute to or precede the final pass.
    let mut reachable = BTreeSet::new();
    let mut visit = vec![final_original];
    while let Some(index) = visit.pop() {
        if reachable.insert(index) {
            visit.extend(&dependencies[index]);
        }
    }
    if reachable.len() != passes.len() {
        return Err(format!(
            "{}: effect graph has passes disconnected from its final output",
            passes
                .iter()
                .enumerate()
                .filter(|(i, _)| !reachable.contains(i))
                .map(|(_, p)| p.owner.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut done = BTreeSet::new();
    let mut order = Vec::new();
    while done.len() < passes.len() {
        let next = passes
            .iter()
            .enumerate()
            .filter(|(i, _)| !done.contains(i) && dependencies[*i].is_subset(&done))
            .min_by_key(|(_, p)| (p.descriptor.order, &p.owner))
            .map(|(i, _)| i)
            .ok_or_else(|| {
                format!(
                    "{}: cyclic effect dependencies",
                    passes
                        .iter()
                        .map(|p| p.owner.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        order.push(next);
        done.insert(next);
    }
    let sorted = order.iter().map(|i| passes[*i].clone()).collect::<Vec<_>>();
    let final_index = order.iter().position(|i| *i == final_original).unwrap();
    Ok(Prepared {
        #[cfg(test)]
        owner: sorted[final_index].owner.clone(),
        passes: sorted,
        final_index,
    })
}
