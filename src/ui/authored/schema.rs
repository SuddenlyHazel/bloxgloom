use super::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Panel,
    Label,
    Image,
    Button,
    Input,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Style {
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
    #[serde(deserialize_with = "bounded_nodes")]
    nodes: Vec<RawNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Presentation {
    capability: String,
    module: String,
}

// Reject the 65th node before deserializing/allocating its payload. The flat
// parent-index tree also avoids recursive document parsing and traversal.
fn bounded_nodes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<RawNode>, D::Error> {
    struct Nodes;
    impl<'de> serde::de::Visitor<'de> for Nodes {
        type Value = Vec<RawNode>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 64 UI nodes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut nodes = Vec::new();
            while nodes.len() < 64 {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    id: String,
    parent: Option<usize>,
    kind: Kind,
    style: String,
    #[serde(default)]
    text: String,
    image: Option<String>,
    event: Option<String>,
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
        if self.version != 1 || self.nodes.is_empty() {
            return Err(INVALID);
        }
        let mut nodes: Vec<Widget> = Vec::new();
        let mut depths = Vec::new();
        let mut ids = std::collections::BTreeSet::new();
        for (index, raw) in self.nodes.into_iter().enumerate() {
            if !identifier(&raw.id)
                || !ids.insert(raw.id.clone())
                || !owned(owner, &raw.style)
                || raw.text.len() > MAX_TEXT
                || !raw.text.bytes().all(|c| (32..=126).contains(&c))
                || raw.event.as_ref().is_some_and(|id| !owned(owner, id))
            {
                return Err(INVALID);
            }
            let depth = match raw.parent {
                None if index == 0 && raw.kind == Kind::Panel => 1,
                Some(parent) if parent < index && nodes[parent].kind == Kind::Panel => {
                    depths[parent] + 1
                }
                _ => return Err(INVALID),
            };
            if depth > 16 {
                return Err(INVALID);
            }
            let style = styles.get(&raw.style).ok_or(INVALID)?.clone();
            let textual = matches!(raw.kind, Kind::Label | Kind::Button | Kind::Input);
            if (textual && style.font.is_none())
                || (!textual && !raw.text.is_empty())
                || (raw.event.is_some() && !matches!(raw.kind, Kind::Button | Kind::Input))
                || (raw.kind == Kind::Image) != raw.image.is_some()
                || raw
                    .image
                    .as_ref()
                    .is_some_and(|id| !owned(owner, id) || !images.contains_key(id))
            {
                return Err(INVALID);
            }
            depths.push(depth);
            nodes.push(Widget {
                id: format!("{owner}:{local}/{}", raw.id),
                parent: raw.parent,
                kind: raw.kind,
                style,
                text: raw.text,
                image: raw.image,
                event: raw.event,
            });
        }
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
            nodes,
            script,
        })
    }
}
