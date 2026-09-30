use super::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Panel,
    Label,
    Image,
    Button,
    Input,
    Checkbox,
    Slider,
    Select,
    MultilineInput,
    Table,
    ScrollPanel,
}

impl Kind {
    pub(crate) fn input(self) -> bool {
        matches!(
            self,
            Self::Input | Self::MultilineInput | Self::Checkbox | Self::Slider | Self::Select
        )
    }
    pub(crate) fn textual(self) -> bool {
        self.input() || matches!(self, Self::Label | Self::Button)
    }
    pub(crate) fn container(self) -> bool {
        matches!(self, Self::Panel | Self::Table | Self::ScrollPanel)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Style {
    #[serde(default)]
    pub(super) width: u16,
    pub(super) height: u16,
    #[serde(default)]
    pub(super) padding: u16,
    #[serde(default)]
    pub(super) gap: u16,
    #[serde(default)]
    pub(super) row: bool,
    #[serde(default)]
    pub(super) background: [u8; 4],
    #[serde(default = "white")]
    pub(super) color: [u8; 4],
    pub(super) font: Option<String>,
}

fn white() -> [u8; 4] {
    [255; 4]
}

impl Style {
    pub(super) fn validate(&self, owner: &str) -> Result<()> {
        if self.width > 1024
            || self.height == 0
            || self.height > 1024
            || self.padding > 32
            || self.gap > 32
            || self.font.as_ref().is_some_and(|id| !owned(owner, id))
        {
            return Err(INVALID);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDocument {
    version: u8,
    presentation: Option<Presentation>,
    bindings: Option<Vec<super::bindings::Declaration>>,
    #[serde(deserialize_with = "bounded_nodes")]
    nodes: Vec<RawNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Presentation {
    capability: String,
    module: String,
}

// Reject the 257th node before deserializing/allocating its payload. The flat
// parent-index tree also avoids recursive document parsing and traversal.
fn bounded_nodes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<RawNode>, D::Error> {
    struct Nodes;
    impl<'de> serde::de::Visitor<'de> for Nodes {
        type Value = Vec<RawNode>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 256 UI nodes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut nodes = Vec::new();
            while nodes.len() < 256 {
                let Some(node) = seq.next_element()? else {
                    return Ok(nodes);
                };
                nodes.push(node);
            }
            // A zero-allocation marker errors on any extra value.
            #[derive(Deserialize)]
            enum Never {}
            let _: Option<Never> = seq.next_element()?;
            Ok(nodes)
        }
    }
    d.deserialize_seq(Nodes)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawNode {
    pub(crate) id: String,
    pub(crate) parent: Option<usize>,
    pub(crate) kind: Kind,
    pub(crate) style: String,
    #[serde(default)]
    pub(crate) text: String,
    pub(crate) image: Option<String>,
    pub(crate) event: Option<String>,
    pub(crate) checked: Option<bool>,
    pub(crate) value: Option<f64>,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    pub(crate) step: Option<f64>,
    pub(crate) options: Option<Vec<super::controls::SelectOption>>,
    pub(crate) selected: Option<String>,
}

impl RawDocument {
    pub(super) fn resolve(
        self,
        owner: &str,
        local: &str,
        package: &ClientPackage,
        styles: &BTreeMap<String, Style>,
        images: &BTreeMap<String, super::super::UiRect>,
    ) -> Result<Document> {
        if !matches!(self.version, 1 | 2)
            || self.nodes.is_empty()
            || (self.version == 1
                && (self.bindings.as_ref().is_some_and(|b| !b.is_empty())
                    || self.nodes.len() > 64
                    || self.nodes.iter().any(|n| {
                        !matches!(
                            n.kind,
                            Kind::Panel | Kind::Label | Kind::Image | Kind::Button | Kind::Input
                        ) || n.checked.is_some()
                            || n.value.is_some()
                            || n.min.is_some()
                            || n.max.is_some()
                            || n.step.is_some()
                            || n.options.is_some()
                            || n.selected.is_some()
                    })))
        {
            return Err(INVALID);
        }
        let bindings = super::bindings::resolve(owner, self.bindings)?;
        let nodes = resolve_nodes(owner, local, self.nodes, styles, images, false)?;
        let script = self
            .presentation
            .map(|binding| {
                if binding.capability != "local-ui" || !owned(owner, &binding.module) {
                    return Err(INVALID);
                }
                let key = binding.module.split_once(':').ok_or(INVALID)?.1;
                let source = &package.sources.get(key).ok_or(INVALID)?.source;
                Ok(std::sync::Arc::new(crate::client::presentation::Script {
                    module: binding.module,
                    source: source.clone(),
                }))
            })
            .transpose()?;
        Ok(Document {
            id: format!("{owner}:{local}"),
            version: self.version,
            nodes,
            bindings,
            script,
        })
    }
}

pub(crate) fn resolve_nodes(
    owner: &str,
    local: &str,
    raw_nodes: Vec<RawNode>,
    styles: &BTreeMap<String, Style>,
    images: &BTreeMap<String, super::super::UiRect>,
    allow_forest: bool,
) -> Result<Vec<Widget>> {
    if raw_nodes.len() > 256 || (!allow_forest && raw_nodes.is_empty()) {
        return Err(INVALID);
    }
    let mut nodes: Vec<Widget> = Vec::new();
    let mut depths = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for (index, raw) in raw_nodes.into_iter().enumerate() {
        if !identifier(&raw.id)
            || !ids.insert(raw.id.clone())
            || !owned(owner, &raw.style)
            || raw.text.len() > super::controls::Control::limit(raw.kind)
            || raw
                .text
                .chars()
                .any(|c| c.is_control() && !(raw.kind == Kind::MultilineInput && c == '\n'))
            || raw.event.as_ref().is_some_and(|id| !owned(owner, id))
        {
            return Err(INVALID);
        }
        let depth = match raw.parent {
            None if allow_forest || (index == 0 && raw.kind.container()) => 1,
            Some(parent) if parent < index && nodes[parent].kind.container() => depths[parent] + 1,
            _ => return Err(INVALID),
        };
        if depth > 16 {
            return Err(INVALID);
        }
        let style = styles.get(&raw.style).ok_or(INVALID)?.clone();
        if (raw.kind.textual() && style.font.is_none())
            || (!raw.kind.textual() && !raw.text.is_empty())
            || (raw.event.is_some() && !(raw.kind == Kind::Button || raw.kind.input()))
            || (raw.kind == Kind::Image) != raw.image.is_some()
            || raw
                .image
                .as_ref()
                .is_some_and(|id| !owned(owner, id) || !images.contains_key(id))
        {
            return Err(INVALID);
        }
        let control = control(&raw)?;
        depths.push(depth);
        nodes.push(Widget {
            id: format!("{owner}:{local}/{}", raw.id),
            parent: raw.parent,
            kind: raw.kind,
            style,
            text: raw.text,
            image: raw.image,
            event: raw.event,
            control,
        });
    }
    Ok(nodes)
}

fn control(raw: &RawNode) -> Result<super::controls::Control> {
    use super::controls::Control;
    if (raw.checked.is_some() && raw.kind != Kind::Checkbox)
        || ((raw.value.is_some() || raw.min.is_some() || raw.max.is_some() || raw.step.is_some())
            && raw.kind != Kind::Slider)
        || ((raw.options.is_some() || raw.selected.is_some()) && raw.kind != Kind::Select)
    {
        return Err(INVALID);
    }
    Ok(match raw.kind {
        Kind::Checkbox => Control::Checkbox {
            checked: raw.checked.unwrap_or(false),
        },
        Kind::Slider => {
            let min = raw.min.unwrap_or(0.0);
            let max = raw.max.unwrap_or(1.0);
            let value = raw.value.unwrap_or(min);
            if !min.is_finite()
                || !max.is_finite()
                || !(max - min).is_finite()
                || min >= max
                || !value.is_finite()
                || value < min
                || value > max
                || raw
                    .step
                    .is_some_and(|n| !n.is_finite() || n <= 0.0 || n > max - min)
            {
                return Err(INVALID);
            }
            let c = Control::Slider {
                value,
                min,
                max,
                step: raw.step,
            };
            if !c.validate(Kind::Slider, &Control::number(value)) {
                return Err(INVALID);
            }
            c
        }
        Kind::Select => {
            let options = raw.options.clone().ok_or(INVALID)?;
            if options.is_empty() || options.len() > 64 {
                return Err(INVALID);
            }
            let mut keys = std::collections::BTreeSet::new();
            for option in &options {
                if !identifier(&option.key)
                    || !keys.insert(&option.key)
                    || option.label.len() > MAX_TEXT
                    || option.label.chars().any(char::is_control)
                {
                    return Err(INVALID);
                }
            }
            let selected = raw
                .selected
                .clone()
                .unwrap_or_else(|| options[0].key.clone());
            if !options.iter().any(|o| o.key == selected) {
                return Err(INVALID);
            }
            Control::Select { options, selected }
        }
        _ => Control::None,
    })
}

#[cfg(test)]
mod control_tests {
    use super::*;
    fn raw(value: serde_json::Value) -> RawNode {
        serde_json::from_value(value).unwrap()
    }
    fn styles() -> BTreeMap<String, Style> {
        BTreeMap::from([(
            "demo:style".into(),
            serde_json::from_str(r#"{"height":32,"font":"demo:font"}"#).unwrap(),
        )])
    }
    #[test]
    fn dynamic_control_forest_validates_choices_ownership_and_container_parents() {
        let forest = vec![
            raw(
                serde_json::json!({"id":"check","kind":"checkbox","style":"demo:style","checked":true,"text":"Ready"}),
            ),
            raw(
                serde_json::json!({"id":"choice","kind":"select","style":"demo:style","options":[{"key":"one","label":"One"}],"selected":"one"}),
            ),
            raw(
                serde_json::json!({"id":"notes","kind":"multiline_input","style":"demo:style","text":"first\nsecond"}),
            ),
        ];
        let resolved = resolve_nodes(
            "demo",
            "panel",
            forest.clone(),
            &styles(),
            &BTreeMap::new(),
            true,
        )
        .unwrap();
        assert_eq!(resolved[0].control.initial(Kind::Checkbox, ""), "true");
        assert!(
            resolve_nodes("demo", "panel", forest, &styles(), &BTreeMap::new(), false).is_err()
        );
        let bad = raw(
            serde_json::json!({"id":"choice","kind":"select","style":"demo:style","options":[{"key":"one","label":"One"}],"selected":"missing"}),
        );
        assert!(
            resolve_nodes(
                "demo",
                "panel",
                vec![bad],
                &styles(),
                &BTreeMap::new(),
                true
            )
            .is_err()
        );
        let foreign =
            raw(serde_json::json!({"id":"choice","kind":"checkbox","style":"other:style"}));
        assert!(
            resolve_nodes(
                "demo",
                "panel",
                vec![foreign],
                &styles(),
                &BTreeMap::new(),
                true
            )
            .is_err()
        );
        let parent = raw(serde_json::json!({"id":"row","kind":"label","style":"demo:style"}));
        let child =
            raw(serde_json::json!({"id":"cell","parent":0,"kind":"label","style":"demo:style"}));
        assert!(
            resolve_nodes(
                "demo",
                "panel",
                vec![parent, child],
                &styles(),
                &BTreeMap::new(),
                true
            )
            .is_err()
        );
    }
}
