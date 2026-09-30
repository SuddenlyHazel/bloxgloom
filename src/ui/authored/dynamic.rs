//! Atomic runtime tree replacement against the session's frozen resource set.
use super::*;
use crate::client::presentation::{Command, ControlValue};

pub(super) struct Candidate {
    document: Document,
    texts: Vec<String>,
    inputs: Vec<String>,
    visible: Vec<bool>,
    focus: Option<String>,
    changed: bool,
}
impl Session {
    pub(crate) fn node_index(&self, id: &str) -> Option<usize> {
        self.document().nodes.iter().position(|node| node.id == id)
    }
    pub(super) fn control_values(&self) -> Vec<(String, ControlValue)> {
        self.document()
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind.input())
            .map(|(i, n)| {
                let text = &self.inputs[i];
                let value = match n.control {
                    controls::Control::Checkbox { .. } => ControlValue::Bool(text == "true"),
                    controls::Control::Slider { .. } => {
                        ControlValue::Number(text.parse().unwrap_or(0.0))
                    }
                    _ => ControlValue::Text(text.clone()),
                };
                (n.id.clone(), value)
            })
            .collect()
    }
    pub(super) fn prepare_widgets(
        &self,
        commands: &[Command],
        allowed: bool,
    ) -> std::result::Result<Candidate, String> {
        let mut candidate = Candidate {
            document: self.document().clone(),
            texts: self.texts.clone(),
            inputs: self.inputs.clone(),
            visible: self.visible.clone(),
            focus: self.focused_id().map(str::to_owned),
            changed: false,
        };
        for command in commands {
            let id = match command {
                Command::Text(id, _)
                | Command::Value(id, _)
                | Command::Visible(id, _)
                | Command::Children(id, _) => id,
                _ => continue,
            };
            if !allowed {
                return Err("UI reply does not own active document".into());
            }
            let i = candidate
                .document
                .nodes
                .iter()
                .position(|n| &n.id == id)
                .ok_or("unknown UI node")?;
            let node = &candidate.document.nodes[i];
            match command {
                Command::Children(_, raw) => {
                    if candidate.document.version != 2 || !node.kind.container() {
                        return Err("children requires a version-2 container".into());
                    }
                    candidate.replace(i, raw.clone(), &self.resources)?;
                }
                Command::Text(_, value) => {
                    let valid = if matches!(node.kind, Kind::Input | Kind::MultilineInput) {
                        node.control.validate(node.kind, value)
                    } else {
                        node.kind.textual()
                            && value.len() <= MAX_TEXT
                            && !value.chars().any(char::is_control)
                    };
                    if !valid {
                        return Err("invalid widget text".into());
                    }
                    if matches!(node.kind, Kind::Input | Kind::MultilineInput) {
                        candidate.inputs[i] = value.clone();
                    } else {
                        candidate.texts[i] = value.clone();
                    }
                }
                Command::Value(_, value) => {
                    if candidate.document.version != 2
                        || !node.kind.input()
                        || !node.control.validate(node.kind, value)
                    {
                        return Err("invalid widget value".into());
                    }
                    candidate.inputs[i] = value.clone();
                }
                Command::Visible(_, visible) => candidate.visible[i] = *visible,
                _ => unreachable!(),
            }
        }
        candidate.bounds()?;
        Ok(candidate)
    }
    pub(super) fn commit_widgets(&mut self, candidate: Candidate) {
        self.focused = candidate.focus.as_ref().and_then(|id| {
            candidate
                .document
                .nodes
                .iter()
                .position(|n| &n.id == id && (n.kind.input() || n.kind == Kind::Button))
        });
        self.active = candidate.document;
        self.texts = candidate.texts;
        self.inputs = candidate.inputs;
        self.visible = candidate.visible;
        if candidate.changed {
            self.tree_generation = self.tree_generation.wrapping_add(1);
            self.rects.clear();
            self.clips.clear();
        }
        if self.focused.is_some_and(|i| !self.is_visible(i)) {
            self.focused = None;
        }
    }
}
impl Candidate {
    fn bounds(&self) -> std::result::Result<(), String> {
        let mut depths = Vec::new();
        let mut bytes = 0;
        for (i, node) in self.document.nodes.iter().enumerate() {
            let depth = node.parent.map_or(1, |p| depths[p] + 1);
            depths.push(depth);
            if depth > 16 {
                return Err("runtime UI depth exceeds 16".into());
            }
            bytes += self.texts[i].len()
                + if node.kind.input() {
                    controls::Control::limit(node.kind)
                } else {
                    self.inputs[i].len()
                };
            if let controls::Control::Select { options, .. } = &node.control {
                bytes += options
                    .iter()
                    .map(|o| o.key.len() + o.label.len())
                    .sum::<usize>();
            }
        }
        if self.document.nodes.len() > 256 || bytes > 32 * 1024 {
            return Err("runtime UI exceeds node/text budget".into());
        }
        Ok(())
    }
    fn replace(
        &mut self,
        target: usize,
        raw: Vec<RawNode>,
        resources: &Resources,
    ) -> std::result::Result<(), String> {
        let (owner, local) = self
            .document
            .id
            .split_once(':')
            .ok_or("invalid document identity")?;
        let new = schema::resolve_nodes(
            owner,
            local,
            raw,
            &resources.styles,
            &resources.images,
            true,
        )
        .map_err(str::to_owned)?;
        let old = &self.document.nodes;
        let removed = old
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if i == target {
                    return false;
                }
                let mut p = n.parent;
                while let Some(index) = p {
                    if index == target {
                        return true;
                    }
                    p = old[index].parent;
                }
                false
            })
            .collect::<Vec<_>>();
        let retained = old
            .iter()
            .enumerate()
            .filter(|(i, _)| !removed[*i])
            .map(|(_, n)| n.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if new.iter().any(|n| retained.contains(n.id.as_str())) {
            return Err("dynamic node identity collides".into());
        }
        let mut nodes = Vec::new();
        let mut texts = Vec::new();
        let mut inputs = Vec::new();
        let mut visible = Vec::new();
        let mut mapping = vec![None; old.len()];
        for (i, node) in old.iter().enumerate() {
            if removed[i] {
                continue;
            }
            let mut node = node.clone();
            node.parent = node.parent.map(|p| mapping[p].expect("retained ancestor"));
            mapping[i] = Some(nodes.len());
            nodes.push(node);
            texts.push(self.texts[i].clone());
            inputs.push(self.inputs[i].clone());
            visible.push(self.visible[i]);
            if i == target {
                let base = nodes.len();
                let parent = mapping[i].unwrap();
                for node in &new {
                    let mut node = node.clone();
                    node.parent = Some(node.parent.map_or(parent, |p| base + p));
                    let previous = old
                        .iter()
                        .position(|n| n.id == node.id && n.kind == node.kind);
                    let input = previous
                        .filter(|p| {
                            node.kind.input() && node.control.validate(node.kind, &self.inputs[*p])
                        })
                        .map_or_else(
                            || node.control.initial(node.kind, &node.text),
                            |p| self.inputs[p].clone(),
                        );
                    texts.push(node.text.clone());
                    inputs.push(if node.kind.input() {
                        input
                    } else {
                        String::new()
                    });
                    visible.push(previous.is_none_or(|p| self.visible[p]));
                    nodes.push(node);
                }
            }
        }
        self.document.nodes = nodes;
        self.texts = texts;
        self.inputs = inputs;
        self.visible = visible;
        self.changed = true;
        self.bounds()
    }
}
