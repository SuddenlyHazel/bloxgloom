//! Inventory slot painting shared by the player grid and authored machine views.

use crate::{content::Catalog, inventory::Stack};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{GOLD, MUTED, PANEL_EDGE, SLOT, SLOT_HOVER, TEXT};

pub(crate) struct SlotStyle {
    pub side: f32,
    pub selected: bool,
    pub highlighted: bool,
    pub dimmed: bool,
    pub hotbar: bool,
}

pub(crate) fn show(
    ui: &mut Ui,
    number: u8,
    stack: Option<&Stack>,
    catalog: &Catalog,
    style: SlotStyle,
) -> egui::Response {
    let SlotStyle {
        side,
        selected,
        highlighted,
        dimmed,
        hotbar,
    } = style;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    let hovered = response.hovered();
    let fill = if selected {
        Color32::from_rgb(62, 67, 45)
    } else if hovered {
        SLOT_HOVER
    } else {
        SLOT
    };
    let border = if selected || highlighted {
        GOLD
    } else if hovered {
        Color32::from_rgb(172, 195, 156)
    } else {
        PANEL_EDGE
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(if selected { 2.0 } else { 1.0 }, border),
        StrokeKind::Inside,
    );
    if let Some(stack) = stack {
        paint_icon(ui, rect, stack, catalog);
        let count = stack.count.to_string();
        let count_size = (side * 0.24).clamp(9.0, 13.0);
        let count_pos = Pos2::new(rect.right() - 4.0, rect.bottom() - 3.0);
        painter.text(
            count_pos + Vec2::new(1.0, 1.0),
            Align2::RIGHT_BOTTOM,
            &count,
            FontId::monospace(count_size),
            Color32::BLACK,
        );
        painter.text(
            count_pos,
            Align2::RIGHT_BOTTOM,
            count,
            FontId::monospace(count_size),
            TEXT,
        );
    } else if side >= 34.0 {
        painter.circle_filled(rect.center(), 1.5, PANEL_EDGE);
    }
    if hotbar {
        painter.text(
            rect.left_top() + Vec2::new(5.0, 4.0),
            Align2::LEFT_TOP,
            (number + 1).to_string(),
            FontId::monospace((side * 0.19).clamp(8.0, 11.0)),
            if selected { GOLD } else { MUTED },
        );
    }
    if dimmed {
        painter.rect_filled(rect.shrink(1.0), 4.0, Color32::from_black_alpha(118));
    }
    let name = stack
        .and_then(|stack| catalog.item(stack.item))
        .map_or("Empty", |item| item.name.as_ref());
    let count = stack.map_or(String::new(), |stack| format!("  ×{}", stack.count));
    response.on_hover_text(format!("Slot {}  ·  {name}{count}", number + 1))
}

fn paint_icon(ui: &Ui, rect: Rect, stack: &Stack, catalog: &Catalog) {
    let side = rect.width();
    let icon_side = side * 0.55;
    let icon_rect = Rect::from_center_size(
        rect.center() - Vec2::new(0.0, side * 0.045),
        Vec2::splat(icon_side),
    );
    let painter = ui.painter();
    if let Some(icon) = catalog.item_icon(stack.item) {
        let columns = icon.rows.iter().map(String::len).max().unwrap_or(1) as f32;
        let cell_w = icon_rect.width() / columns;
        let cell_h = icon_rect.height() / icon.rows.len() as f32;
        for (y, row) in icon.rows.iter().enumerate() {
            let x0 = icon_rect.left() + (columns - row.len() as f32) * cell_w * 0.5;
            for (x, symbol) in row.bytes().enumerate() {
                if let Some((_, rgba)) = icon.palette.iter().find(|(key, _)| *key == symbol) {
                    let color = color_from_swatch(*rgba);
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(x0 + x as f32 * cell_w, icon_rect.top() + y as f32 * cell_h),
                            Vec2::new(cell_w + 0.3, cell_h + 0.3),
                        ),
                        0.0,
                        color,
                    );
                }
            }
        }
    } else {
        let color = catalog
            .item(stack.item)
            .map_or(Color32::from_rgb(148, 102, 169), |item| {
                color_from_swatch(item.swatch)
            });
        painter.rect_filled(icon_rect, 2.0, color);
        painter.line_segment(
            [icon_rect.left_top(), icon_rect.right_top()],
            Stroke::new(2.0, Color32::from_white_alpha(65)),
        );
        painter.line_segment(
            [icon_rect.right_top(), icon_rect.right_bottom()],
            Stroke::new(2.0, Color32::from_black_alpha(72)),
        );
    }
}

fn color_from_swatch(rgba: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (rgba[0] * 255.0) as u8,
        (rgba[1] * 255.0) as u8,
        (rgba[2] * 255.0) as u8,
        (rgba[3] * 255.0) as u8,
    )
}
