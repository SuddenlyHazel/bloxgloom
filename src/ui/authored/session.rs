//! Bounded local presentation state. No network/gameplay authority.
use super::super::UiRect;
use super::*;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Arc;
use taffy::prelude::*;

#[derive(Debug)]
pub(crate) struct Session {
    pub(super) resources: Arc<Resources>,
    document: usize,
    pub(super) document_generation: u64,
    pub(super) rects: Vec<UiRect>,
    pub(super) clips: Vec<UiRect>,
    pub(super) inputs: Vec<String>,
    pub(super) focused: Option<usize>,
    pub(super) scale: f32,
    pub(super) texts: Vec<String>,
    pub(super) visible: Vec<bool>,
    pub(super) state: String,
    pub(super) worker: Option<crate::client::presentation::Worker>,
    pub(super) sequence: u32,
    pub(super) pending: Option<u32>,
    pub(super) expected: Option<u32>,
    pub(super) failure: Option<String>,
    pub(super) action: Option<String>,
    pub(super) in_flight: Option<(u128, u64)>,
    pub(super) feedback: Option<String>,
    pub(super) replica_events:
        VecDeque<(String, String, Vec<crate::client::presentation::EntityView>)>,
    pub(super) visual_poses: BTreeMap<u64, [f32; 3]>,
    pub(super) visual_tints: BTreeMap<u64, [f32; 3]>,
    pub(super) effects: crate::client::presentation::EffectBuffer,
    pub(super) replica_previous: Vec<u64>,
    pub(super) startup: crate::client::startup::State,
}

impl Session {
    pub(crate) fn new(resources: Arc<Resources>) -> Self {
        Self::with_startup(resources, Default::default())
    }

    pub(crate) fn with_startup(
        resources: Arc<Resources>,
        startup: crate::client::startup::State,
    ) -> Self {
        let worker = (resources.documents.iter().any(|d| d.script.is_some())
            || startup.replica.is_some())
        .then(crate::client::presentation::Worker::spawn)
        .transpose();
        let (worker, failure) = match worker {
            Ok(worker) => (worker, None),
            Err(error) => (None, Some(format!("presentation worker: {error}"))),
        };
        let mut session = Self {
            resources,
            document: 0,
            document_generation: 0,
            rects: Vec::new(),
            clips: Vec::new(),
            inputs: Vec::new(),
            focused: None,
            scale: 1.0,
            texts: Vec::new(),
            visible: Vec::new(),
            state: String::new(),
            worker,
            sequence: 0,
            pending: None,
            expected: None,
            failure: None,
            action: None,
            in_flight: None,
            feedback: None,
            replica_events: VecDeque::new(),
            visual_poses: BTreeMap::new(),
            visual_tints: BTreeMap::new(),
            effects: Default::default(),
            replica_previous: Vec::new(),
            startup,
        };
        session.reset();
        session.failure = failure;
        session
    }

    fn reset(&mut self) {
        // Invalidate an outstanding result without admitting a second job.
        self.document_generation = self.document_generation.wrapping_add(1);
        self.expected = None;
        self.replica_events.clear();
        self.visual_poses.clear();
        self.visual_tints.clear();
        self.effects.clear();
        self.replica_previous.clear();
        self.failure = None;
        self.action = None;
        self.feedback = None;
        self.state.clear();
        self.texts = self
            .document()
            .nodes
            .iter()
            .map(|n| n.text.clone())
            .collect();
        for (index, node) in self.resources.documents[self.document]
            .nodes
            .iter()
            .enumerate()
        {
            if let Some(text) = self.startup.texts.get(&node.id) {
                self.texts[index] = text.clone();
            }
        }
        if let Some(value) = self.startup.states.get(&self.document().id) {
            self.state = value.clone();
        }
        self.visible = vec![true; self.document().nodes.len()];
        self.focused = None;
        self.rects.clear();
        self.clips.clear();
        self.inputs = self
            .document()
            .nodes
            .iter()
            .map(|n| {
                if n.kind == Kind::Input {
                    n.text.clone()
                } else {
                    String::new()
                }
            })
            .collect();
        for (index, node) in self.resources.documents[self.document]
            .nodes
            .iter()
            .enumerate()
        {
            if node.kind == Kind::Input
                && let Some(text) = self.startup.texts.get(&node.id)
            {
                self.inputs[index] = text.clone();
            }
        }
    }

    pub(crate) fn next_document(&mut self) {
        self.document = (self.document + 1) % self.resources.documents.len();
        self.reset();
    }

    pub(crate) fn resources(&self) -> &Arc<Resources> {
        &self.resources
    }
    pub(super) fn document(&self) -> &Document {
        &self.resources.documents[self.document]
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32, scale: f32) {
        let viewport = UiRect {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        };
        self.scale = super::super::layout::effective_ui_scale(width.max(1), height.max(1), scale);
        let width = (width as f32 / self.scale - 24.0).max(1.0);
        let height = (height as f32 / self.scale - 64.0).max(1.0);
        let nodes = &self.document().nodes;
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut ids = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            let s = &node.style;
            let w = if s.width == 0 {
                Dimension::auto()
            } else {
                length(f32::from(s.width))
            };
            let style = taffy::Style {
                size: Size {
                    width: if index == 0 {
                        length(if s.width == 0 {
                            width
                        } else {
                            f32::from(s.width).min(width)
                        })
                    } else {
                        w
                    },
                    height: length(if index == 0 {
                        f32::from(s.height).min(height)
                    } else {
                        f32::from(s.height)
                    }),
                },
                flex_direction: if s.row {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                flex_shrink: 0.0,
                padding: Rect {
                    left: length(f32::from(s.padding)),
                    right: length(f32::from(s.padding)),
                    top: length(f32::from(s.padding)),
                    bottom: length(f32::from(s.padding)),
                },
                gap: Size {
                    width: length(f32::from(s.gap)),
                    height: length(f32::from(s.gap)),
                },
                ..Default::default()
            };
            let id = tree.new_leaf(style).expect("bounded validated layout");
            if let Some(parent) = node.parent {
                tree.add_child(ids[parent], id).expect("earlier parent");
            }
            ids.push(id);
        }
        tree.compute_layout(
            ids[0],
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
        )
        .expect("bounded layout");
        let root = tree.layout(ids[0]).unwrap();
        let origin = [
            (width - root.size.width) * 0.5 + 12.0,
            (height - root.size.height) * 0.5 + 40.0,
        ];
        let mut rects: Vec<UiRect> = Vec::with_capacity(nodes.len());
        let mut clips = Vec::with_capacity(nodes.len());
        for (node, id) in nodes.iter().zip(ids) {
            let layout = tree.layout(id).unwrap();
            let [x, y] = node
                .parent
                .map_or([origin[0] * self.scale, origin[1] * self.scale], |p| {
                    [rects[p].x, rects[p].y]
                });
            let rect = UiRect {
                x: x + layout.location.x * self.scale,
                y: y + layout.location.y * self.scale,
                width: layout.size.width * self.scale,
                height: layout.size.height * self.scale,
            };
            let clip = intersect(rect, node.parent.map_or(viewport, |p| clips[p]));
            rects.push(rect);
            clips.push(clip);
        }
        self.rects = rects;
        self.clips = clips;
        if self.focused.is_some_and(|i| !self.focusable(i)) {
            self.focused = None;
        }
    }

    fn focusable(&self, i: usize) -> bool {
        self.is_visible(i)
            && matches!(self.document().nodes[i].kind, Kind::Button | Kind::Input)
            && self
                .clips
                .get(i)
                .is_some_and(|r| r.width > 0.0 && r.height > 0.0)
    }

    pub(crate) fn tab(&mut self, backwards: bool) {
        let count = self.document().nodes.len();
        let start = self
            .focused
            .unwrap_or(if backwards { 0 } else { count - 1 });
        self.focused = (1..=count)
            .map(|step| {
                if backwards {
                    (start + count - step) % count
                } else {
                    (start + step) % count
                }
            })
            .find(|&i| self.focusable(i));
    }

    pub(crate) fn click(&mut self, x: f32, y: f32) {
        self.focused = (0..self.document().nodes.len())
            .rev()
            .find(|&i| self.focusable(i) && self.clips[i].contains(x, y));
        self.activate();
    }

    pub(crate) fn edit(&mut self, backspace: bool, text: Option<&str>) {
        let Some(i) = self
            .focused
            .filter(|&i| self.document().nodes[i].kind == Kind::Input)
        else {
            return;
        };
        if !self.can_dispatch(i) {
            return;
        }
        let old = self.inputs[i].clone();
        if backspace {
            self.inputs[i].pop();
        } else if let Some(text) = text {
            // Bound traversal as well as retained input, including non-ASCII paste.
            for byte in text.bytes().take(MAX_TEXT) {
                if (32..=126).contains(&byte) && self.inputs[i].len() < MAX_TEXT {
                    self.inputs[i].push(char::from(byte));
                }
            }
        }
        if self.inputs[i] != old && !self.dispatch(i) {
            self.inputs[i] = old;
        }
    }

    /// Stable package-scoped authored handler identity, never a gameplay command.
    pub(crate) fn event(&self) -> Option<&str> {
        self.focused
            .and_then(|i| self.document().nodes[i].event.as_deref())
    }

    pub(crate) fn focused_id(&self) -> Option<&str> {
        self.focused.map(|i| self.document().nodes[i].id.as_str())
    }
}

pub(super) fn intersect(a: UiRect, b: UiRect) -> UiRect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    UiRect {
        x,
        y,
        width: ((a.x + a.width).min(b.x + b.width) - x).max(0.0),
        height: ((a.y + a.height).min(b.y + b.height) - y).max(0.0),
    }
}
