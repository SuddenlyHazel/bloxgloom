//! Resolve script phase edges across the complete, frozen declaration bundle.
use bloxgloom_host_api::{composition::Package, system::System};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(packages: &[Package], systems: &[System]) -> std::io::Result<()> {
    let keys = systems
        .iter()
        .map(|s| s.key.as_str())
        .collect::<BTreeSet<_>>();
    let mut edges = BTreeMap::new();
    for system in systems {
        let owner = system.key.split_once(':').expect("validated key").0;
        let package = packages
            .iter()
            .find(|p| p.key == format!("{owner}:package"))
            .expect("startup package");
        for target in &system.after {
            let target_owner = target.split_once(':').expect("validated edge").0;
            if target_owner != owner
                && !package
                    .dependencies
                    .iter()
                    .any(|d| d.package == format!("{target_owner}:package"))
            {
                return Err(std::io::Error::other(format!(
                    "system {} after {target}: target must belong to the same package or an explicitly declared direct dependency",
                    system.key
                )));
            }
            if !keys.contains(target.as_str()) {
                return Err(std::io::Error::other(format!(
                    "system {} after {target}: missing registered system target",
                    system.key
                )));
            }
        }
        edges.insert(
            system.key.as_str(),
            system.after.iter().map(String::as_str).collect::<Vec<_>>(),
        );
    }
    let mut done = BTreeSet::new();
    let mut path = Vec::new();
    for key in edges.keys() {
        visit(key, &edges, &mut done, &mut path)?;
    }
    Ok(())
}

fn visit<'a>(
    key: &'a str,
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
    done: &mut BTreeSet<&'a str>,
    path: &mut Vec<&'a str>,
) -> std::io::Result<()> {
    if done.contains(key) {
        return Ok(());
    }
    if let Some(start) = path.iter().position(|node| *node == key) {
        let mut cycle = path[start..].to_vec();
        cycle.push(key);
        return Err(std::io::Error::other(format!(
            "system after dependency cycle: {}",
            cycle.join(" -> ")
        )));
    }
    path.push(key);
    for target in &edges[key] {
        visit(target, edges, done, path)?;
    }
    path.pop();
    done.insert(key);
    Ok(())
}
