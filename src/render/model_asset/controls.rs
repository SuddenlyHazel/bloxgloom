//! Named mesh variants, independent visibility toggles and material color inputs.
use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Controls {
    #[serde(default)]
    pub variants: Vec<VariantGroup>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub tints: Vec<Tint>,
    /// glTF does not standardize loop intent; the asset settings supply it.
    #[serde(default)]
    pub loops: BTreeMap<String, bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VariantGroup {
    pub name: String,
    pub default: String,
    pub options: Vec<Variant>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Variant {
    pub name: String,
    pub nodes: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layer {
    pub name: String,
    pub nodes: Vec<String>,
    pub visible: bool,
}
#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TintMode {
    #[default]
    Multiply,
    Replace,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Color {
    pub rgb: [u8; 3],
    #[serde(default)]
    pub mode: TintMode,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tint {
    pub name: String,
    pub materials: Vec<String>,
    pub color: Color,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Look {
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default)]
    pub layers: BTreeMap<String, bool>,
    #[serde(default)]
    pub tints: BTreeMap<String, Color>,
}
pub(crate) struct Appearance {
    pub visible: Vec<bool>,
    /// Linear RGB customization color and replacement flag, per material.
    pub colors: Vec<[f32; 4]>,
}
fn control_name(value: &str) -> Result<()> {
    ensure(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "invalid customization control name",
    )
}
fn resolve(names: &[String], targets: impl Iterator<Item = String>) -> Result<Vec<usize>> {
    ensure(
        !names.is_empty() && names.len() <= 256,
        "control needs 1..256 targets",
    )?;
    let targets: Vec<_> = targets.collect();
    let mut ids = Vec::new();
    for name in names {
        let matches: Vec<_> = targets
            .iter()
            .enumerate()
            .filter(|(_, n)| *n == name)
            .map(|(i, _)| i)
            .collect();
        ensure(
            matches.len() == 1,
            &format!("missing or ambiguous model target: {name}"),
        )?;
        ensure(!ids.contains(&matches[0]), "duplicate customization target")?;
        ids.push(matches[0]);
    }
    Ok(ids)
}
impl Controls {
    pub(crate) fn validate(&self, model: &Model) -> Result<()> {
        let mut names = HashSet::new();
        let mut nodes = HashSet::new();
        let mut materials = HashSet::new();
        ensure(
            self.variants.len() + self.layers.len() + self.tints.len() <= 64,
            "more than 64 appearance controls",
        )?;
        for name in self
            .variants
            .iter()
            .map(|v| &v.name)
            .chain(self.layers.iter().map(|v| &v.name))
            .chain(self.tints.iter().map(|v| &v.name))
        {
            control_name(name)?;
            ensure(names.insert(name), "duplicate appearance control name")?;
        }
        for group in &self.variants {
            ensure(
                !group.options.is_empty() && group.options.len() <= 32,
                "variant group needs 1..32 options",
            )?;
            let mut options = HashSet::new();
            for option in &group.options {
                control_name(&option.name)?;
                ensure(options.insert(&option.name), "duplicate variant name")?;
                for node in resolve(&option.nodes, model.nodes.iter().map(|n| n.name.clone()))? {
                    ensure(
                        nodes.insert(node),
                        "appearance node belongs to multiple controls",
                    )?;
                }
            }
            ensure(options.contains(&group.default), "unknown default variant")?;
        }
        for layer in &self.layers {
            for node in resolve(&layer.nodes, model.nodes.iter().map(|n| n.name.clone()))? {
                ensure(
                    nodes.insert(node),
                    "appearance node belongs to multiple controls",
                )?;
            }
        }
        for tint in &self.tints {
            for material in resolve(
                &tint.materials,
                model.materials.iter().map(|m| m.name.clone()),
            )? {
                ensure(
                    materials.insert(material),
                    "material belongs to multiple tint controls",
                )?;
            }
        }
        for name in self.loops.keys() {
            ensure(
                model.clips.iter().any(|c| &c.name == name),
                &format!("loop setting references unknown clip: {name}"),
            )?;
        }
        Ok(())
    }
}
impl Model {
    pub(crate) fn appearance(&self, look: &Look) -> Result<Appearance> {
        for name in look.variants.keys() {
            ensure(
                self.controls.variants.iter().any(|g| &g.name == name),
                "unknown variant control",
            )?;
        }
        for name in look.layers.keys() {
            ensure(
                self.controls.layers.iter().any(|g| &g.name == name),
                "unknown layer control",
            )?;
        }
        for name in look.tints.keys() {
            ensure(
                self.controls.tints.iter().any(|g| &g.name == name),
                "unknown tint control",
            )?;
        }
        let mut visible = vec![true; self.nodes.len()];
        for group in &self.controls.variants {
            let selected = look.variants.get(&group.name).unwrap_or(&group.default);
            ensure(
                group.options.iter().any(|o| &o.name == selected),
                "unknown variant option",
            )?;
            for option in &group.options {
                for id in resolve(&option.nodes, self.nodes.iter().map(|n| n.name.clone()))? {
                    visible[id] = &option.name == selected;
                }
            }
        }
        for layer in &self.controls.layers {
            let value = look
                .layers
                .get(&layer.name)
                .copied()
                .unwrap_or(layer.visible);
            for id in resolve(&layer.nodes, self.nodes.iter().map(|n| n.name.clone()))? {
                visible[id] = value;
            }
        }
        // Hiding a group hides its subtree without deleting joints or clips.
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                visible[i] &= visible[parent];
            }
        }
        let mut colors = vec![[1.0, 1.0, 1.0, 0.0]; self.materials.len()];
        for tint in &self.controls.tints {
            let color = look.tints.get(&tint.name).copied().unwrap_or(tint.color);
            let rgb = color.rgb.map(|v| {
                let v = f32::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            for id in resolve(
                &tint.materials,
                self.materials.iter().map(|m| m.name.clone()),
            )? {
                colors[id] = [
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    f32::from(color.mode == TintMode::Replace),
                ];
            }
        }
        Ok(Appearance { visible, colors })
    }
}
