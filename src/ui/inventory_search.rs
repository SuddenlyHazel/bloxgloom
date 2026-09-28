//! One non-authoritative inventory search field and its flexbox header geometry.
use super::types::UiRect;
use taffy::prelude::*;
use winit::keyboard::KeyCode;

#[derive(Default)]
pub(crate) struct InventorySearch(String);

impl InventorySearch {
    pub fn text(&self) -> &str {
        &self.0
    }

    pub fn edit(&mut self, key: KeyCode, text: Option<&str>) {
        match key {
            KeyCode::Backspace => {
                self.0.pop();
            }
            KeyCode::Delete => self.0.clear(),
            _ => {
                if let Some(text) = text {
                    for ch in text
                        .chars()
                        .filter(|ch| ch.is_ascii_graphic() || *ch == ' ')
                    {
                        if self.0.len() < 20 {
                            self.0.push(ch);
                        }
                    }
                }
            }
        }
    }
}

/// Lay out the inventory header with Taffy's flexbox; slot geometry remains unchanged.
pub(super) fn search_rect(panel: UiRect, scale: f32) -> UiRect {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let title = tree
        .new_leaf(Style {
            flex_grow: 1.0,
            ..Default::default()
        })
        .expect("title node");
    let search = tree
        .new_leaf(Style {
            size: Size {
                width: percent(0.48),
                height: percent(1.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        })
        .expect("search node");
    let root = tree
        .new_with_children(
            Style {
                size: Size {
                    width: length((panel.width - 44.0 * scale).max(1.0)),
                    height: length(32.0 * scale),
                },
                display: Display::Flex,
                ..Default::default()
            },
            &[title, search],
        )
        .expect("header node");
    tree.compute_layout(root, Size::MAX_CONTENT)
        .expect("header layout");
    let rect = tree.layout(search).expect("search layout");
    UiRect {
        x: panel.x + 22.0 * scale + rect.location.x,
        y: panel.y + 12.0 * scale + rect.location.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}
