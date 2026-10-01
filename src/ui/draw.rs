//! CPU-side UI drawing and bitmap font geometry.

use crate::content::Catalog;
use crate::items::ItemId;
use bytemuck::{Pod, Zeroable};
use font8x8::{BASIC_FONTS, UnicodeFonts};

use super::{
    layout::UiLayout,
    types::{SettingId, UiControl, UiDebug, UiFrame, UiRect, UiScreen},
};

mod actions;
mod container;
mod join;
mod screens;

pub(super) const MAX_UI_VERTICES: usize = 32_768;
pub(super) const FONT_WIDTH: u32 = 128;
pub(super) const FONT_HEIGHT: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct UiVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    textured: f32,
}

pub(super) struct UiBuilder<'a> {
    pub(super) vertices: &'a mut Vec<UiVertex>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) scale: f32,
}

impl UiBuilder<'_> {
    pub(super) fn draw_frame(&mut self, frame: &UiFrame<'_>, layout: &UiLayout, catalog: &Catalog) {
        if matches!(frame.screen, UiScreen::Joining | UiScreen::JoinFailed) {
            self.draw_join(frame);
            return;
        }
        self.draw_hud(frame, layout, catalog);
        match frame.screen {
            UiScreen::Playing | UiScreen::Package | UiScreen::Joining | UiScreen::JoinFailed => {}
            UiScreen::Inventory => self.draw_inventory(frame, layout, catalog),
            UiScreen::Container => self.draw_container(frame, layout, catalog),
            UiScreen::Actions => self.draw_actions(frame, layout),
            UiScreen::Admin => self.draw_admin(frame, layout, catalog),
            UiScreen::Pause => self.draw_pause(frame, layout),
            UiScreen::Settings | UiScreen::Graphics | UiScreen::Character => {
                self.draw_settings(frame, layout)
            }
        }
        if let Some(debug) = frame.debug {
            self.draw_debug(debug);
        }
    }

    fn draw_hud(&mut self, frame: &UiFrame<'_>, layout: &UiLayout, catalog: &Catalog) {
        let center_x = self.width * 0.5;
        let center_y = self.height * 0.5;
        if frame.screen == UiScreen::Playing {
            let gap = 4.0 * self.scale;
            let arm = 8.0 * self.scale;
            let thickness = (2.0 * self.scale).max(1.0);
            let shadow = [0.03, 0.04, 0.03, 0.78];
            let white = [0.95, 0.98, 0.91, 0.98];
            // Dark edging keeps the small crosshair visible on bright sky and leaves.
            for (x, y, w, h) in [
                (
                    center_x - gap - arm - 1.0,
                    center_y - 1.0,
                    arm,
                    thickness + 2.0,
                ),
                (center_x + gap + 1.0, center_y - 1.0, arm, thickness + 2.0),
                (
                    center_x - 1.0,
                    center_y - gap - arm - 1.0,
                    thickness + 2.0,
                    arm,
                ),
                (center_x - 1.0, center_y + gap + 1.0, thickness + 2.0, arm),
            ] {
                self.rect(x, y, w, h, shadow);
            }
            for (x, y, w, h) in [
                (
                    center_x - gap - arm,
                    center_y - thickness * 0.5,
                    arm,
                    thickness,
                ),
                (center_x + gap, center_y - thickness * 0.5, arm, thickness),
                (
                    center_x - thickness * 0.5,
                    center_y - gap - arm,
                    thickness,
                    arm,
                ),
                (center_x - thickness * 0.5, center_y + gap, thickness, arm),
            ] {
                self.rect(x, y, w, h, white);
            }
        }
        if frame.screen == UiScreen::Playing {
            for index in 0usize..9 {
                let Some(rect) = layout.rect(UiControl::HotbarSlot(index as u8)) else {
                    continue;
                };
                let selected = index == frame.selected_slot.min(8);
                let bg = if selected {
                    [0.09, 0.12, 0.10, 0.92]
                } else {
                    [0.04, 0.055, 0.06, 0.76]
                };
                self.rounded_panel(
                    rect,
                    bg,
                    if selected {
                        GOLD
                    } else {
                        [0.30, 0.34, 0.31, 0.88]
                    },
                    1.5 * self.scale,
                );
                let swatch = inset(rect, 10.0 * self.scale, 10.0 * self.scale);
                if let Some(stack) = frame.inventory[index].as_ref() {
                    self.draw_item_swatch(swatch, stack.item, catalog);
                    self.text(
                        &stack.count.to_string(),
                        rect.x + 3.0 * self.scale,
                        rect.y + rect.height - 16.0 * self.scale,
                        0.62,
                        TEXT,
                        3,
                    );
                }
                let number = [b'1' + index as u8];
                self.text(
                    std::str::from_utf8(&number).unwrap_or("?"),
                    rect.x + 5.0 * self.scale,
                    rect.y + 4.0 * self.scale,
                    0.72,
                    if selected { GOLD } else { MUTED },
                    1,
                );
            }
            let selected_item = frame.inventory[frame.selected_slot.min(8)]
                .as_ref()
                .map(|stack| stack.item);
            let label_y = layout
                .rect(UiControl::HotbarSlot(0))
                .map_or(self.height - 86.0 * self.scale, |r| r.y - 29.0 * self.scale);
            self.center_text(
                selected_item.map_or("EMPTY HAND", |item| item_name_for(item, catalog)),
                self.width * 0.5,
                label_y,
                0.96,
                TEXT,
            );
        }
        if matches!(frame.screen, UiScreen::Playing | UiScreen::Admin)
            && let Some(status) = frame.status.filter(|s| !s.is_empty())
        {
            let max_chars =
                ((self.width - 24.0 * self.scale) / (18.0 * self.scale * 0.82)).max(1.0) as usize;
            let shown_chars = status.chars().count().min(max_chars).min(96);
            let shown_width = shown_chars as f32 * 18.0 * self.scale * 0.82;
            self.text(
                status,
                self.width - shown_width - 12.0 * self.scale,
                12.0 * self.scale,
                0.82,
                [0.95, 0.79, 0.47, 1.0],
                max_chars.min(96),
            );
        }
    }

    pub(super) fn draw_item_swatch(&mut self, rect: UiRect, item: ItemId, catalog: &Catalog) {
        let Some(icon) = catalog.item_icon(item) else {
            self.rect(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                item_color_for(item, catalog),
            );
            return;
        };
        let rows = &icon.rows;
        let columns = rows.iter().map(|row| row.len()).max().unwrap_or(1) as f32;
        let cell_w = rect.width / columns;
        let cell_h = rect.height / rows.len() as f32;
        for (y, row) in rows.iter().enumerate() {
            let row_offset = (rect.width - row.len() as f32 * cell_w) * 0.5;
            for (x, pixel) in row.bytes().enumerate() {
                let color = icon
                    .palette
                    .iter()
                    .find(|(symbol, _)| *symbol == pixel)
                    .map(|(_, color)| *color);
                if let Some(color) = color {
                    self.rect(
                        rect.x + row_offset + x as f32 * cell_w,
                        rect.y + y as f32 * cell_h,
                        cell_w + 0.5,
                        cell_h + 0.5,
                        color,
                    );
                }
            }
        }
    }

    fn draw_debug(&mut self, debug: UiDebug) {
        let scale = self.scale;
        let x = 12.0 * scale;
        let y = 12.0 * scale;
        let line_h = 19.0 * scale;
        let values = [
            format!(
                "XYZ  {:.1}  {:.1}  {:.1}",
                debug.position[0], debug.position[1], debug.position[2]
            ),
            format!("FPS  {:.0}  /  {:.2} MS", debug.fps, debug.frame_ms),
            format!(
                "CHUNKS  {} VISIBLE  /  {} CACHED",
                debug.visible_chunks, debug.cached_chunks
            ),
            debug
                .latency_ms
                .map_or_else(|| "PING  --".into(), |ms| format!("PING  {ms} MS")),
        ];
        let width = values
            .iter()
            .map(|line| text_width(line, scale, 0.76))
            .fold(0.0, f32::max)
            + 20.0 * scale;
        self.rounded_panel(
            UiRect {
                x: 7.0 * scale,
                y: 7.0 * scale,
                width,
                height: line_h * values.len() as f32 + 14.0 * scale,
            },
            [0.018, 0.027, 0.032, 0.8],
            [0.31, 0.43, 0.39, 0.72],
            scale,
        );
        for (index, line) in values.iter().enumerate() {
            self.text(
                line,
                x,
                y + index as f32 * line_h,
                0.76,
                [0.84, 0.91, 0.82, 0.97],
                96,
            );
        }
    }

    fn screen_dim(&mut self) {
        self.rect(
            0.0,
            0.0,
            self.width,
            self.height,
            [0.012, 0.02, 0.025, 0.69],
        );
    }

    fn panel(&mut self, rect: UiRect) {
        self.rounded_panel(
            rect,
            [0.035, 0.055, 0.056, 1.0],
            [0.28, 0.38, 0.32, 0.94],
            2.0 * self.scale,
        );
        self.rect(
            rect.x + 2.0 * self.scale,
            rect.y + 2.0 * self.scale,
            rect.width - 4.0 * self.scale,
            3.0 * self.scale,
            [0.49, 0.65, 0.36, 0.94],
        );
    }

    fn button(&mut self, rect: UiRect, label: &str, hovered: bool, compact: bool) {
        self.rounded_panel(
            rect,
            if hovered {
                [0.18, 0.25, 0.17, 0.98]
            } else {
                [0.075, 0.10, 0.095, 0.96]
            },
            if hovered {
                [0.72, 0.82, 0.47, 1.0]
            } else {
                EDGE
            },
            if hovered {
                2.0 * self.scale
            } else {
                1.0 * self.scale
            },
        );
        self.center_text(
            label,
            rect.x + rect.width * 0.5,
            rect.y + rect.height * 0.5,
            if compact { 0.84 } else { 0.9 },
            if hovered { GOLD } else { TEXT },
        );
    }

    fn rounded_panel(&mut self, rect: UiRect, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        self.rect(rect.x, rect.y, rect.width, rect.height, fill);
        let t = thickness.max(1.0);
        self.rect(rect.x, rect.y, rect.width, t, border);
        self.rect(rect.x, rect.y + rect.height - t, rect.width, t, border);
        self.rect(rect.x, rect.y, t, rect.height, border);
        self.rect(rect.x + rect.width - t, rect.y, t, rect.height, border);
    }

    pub(in crate::ui) fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) {
        if width <= 0.0 || height <= 0.0 || self.vertices.len() + 6 > MAX_UI_VERTICES {
            return;
        }
        let x0 = x / self.width * 2.0 - 1.0;
        let x1 = (x + width) / self.width * 2.0 - 1.0;
        let y0 = 1.0 - y / self.height * 2.0;
        let y1 = 1.0 - (y + height) / self.height * 2.0;
        self.triangle(
            vertex([x0, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y1], [0.0, 0.0], color, 0.0),
        );
        self.triangle(
            vertex([x0, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y1], [0.0, 0.0], color, 0.0),
            vertex([x0, y1], [0.0, 0.0], color, 0.0),
        );
    }

    pub(in crate::ui) fn text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_scale: f32,
        color: [f32; 4],
        max_chars: usize,
    ) {
        let cell = 16.0 * self.scale * font_scale;
        let advance = 18.0 * self.scale * font_scale;
        let max_chars = max_chars.min(128);
        for (index, ch) in text.chars().take(max_chars).enumerate() {
            let ch = if (ch as u32) < 128 { ch as u8 } else { b'?' };
            let column = u32::from(ch) % 16;
            let row = u32::from(ch) / 16;
            let uv0 = [column as f32 / 16.0, row as f32 / 8.0];
            let uv1 = [(column + 1) as f32 / 16.0, (row + 1) as f32 / 8.0];
            let left = x + index as f32 * advance;
            self.glyph(left, y, cell, cell, uv0, uv1, color);
        }
    }

    fn center_text(
        &mut self,
        text: &str,
        center_x: f32,
        center_y: f32,
        font_scale: f32,
        color: [f32; 4],
    ) {
        let len = text.chars().count().min(128);
        let advance = 18.0 * self.scale * font_scale;
        let width = len as f32 * advance - 2.0 * self.scale * font_scale;
        let height = 16.0 * self.scale * font_scale;
        self.text(
            text,
            center_x - width * 0.5,
            center_y - height * 0.5,
            font_scale,
            color,
            128,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ui) fn glyph(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        if self.vertices.len() + 6 > MAX_UI_VERTICES {
            return;
        }
        let x0 = x / self.width * 2.0 - 1.0;
        let x1 = (x + width) / self.width * 2.0 - 1.0;
        let y0 = 1.0 - y / self.height * 2.0;
        let y1 = 1.0 - (y + height) / self.height * 2.0;
        self.triangle(
            vertex([x0, y0], uv0, color, 1.0),
            vertex([x1, y0], [uv1[0], uv0[1]], color, 1.0),
            vertex([x1, y1], uv1, color, 1.0),
        );
        self.triangle(
            vertex([x0, y0], uv0, color, 1.0),
            vertex([x1, y1], uv1, color, 1.0),
            vertex([x0, y1], [uv0[0], uv1[1]], color, 1.0),
        );
    }

    fn triangle(&mut self, a: UiVertex, b: UiVertex, c: UiVertex) {
        if self.vertices.len() + 3 <= MAX_UI_VERTICES {
            self.vertices.extend_from_slice(&[a, b, c]);
        }
    }
}

pub(super) fn make_font_atlas() -> Vec<u8> {
    let mut pixels = vec![0u8; (FONT_WIDTH * FONT_HEIGHT * 4) as usize];
    for code in 0u8..=127 {
        let Some(glyph) = BASIC_FONTS.get(char::from(code)) else {
            continue;
        };
        let cell_x = u32::from(code) % 16 * 8;
        let cell_y = u32::from(code) / 16 * 8;
        for (y, row) in glyph.into_iter().enumerate() {
            for x in 0..8 {
                let alpha = if row & (1 << x) != 0 { 255 } else { 0 };
                let offset = (((cell_y + y as u32) * FONT_WIDTH + cell_x + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&[255, 255, 255, alpha]);
            }
        }
    }
    pixels
}

fn vertex(position: [f32; 2], uv: [f32; 2], color: [f32; 4], textured: f32) -> UiVertex {
    UiVertex {
        position,
        uv,
        color,
        textured,
    }
}

fn inset(rect: UiRect, x: f32, y: f32) -> UiRect {
    UiRect {
        x: rect.x + x,
        y: rect.y + y,
        width: (rect.width - x * 2.0).max(1.0),
        height: (rect.height - y * 2.0).max(1.0),
    }
}

#[cfg(test)]
pub(super) fn item_name(item: ItemId) -> &'static str {
    item_name_for(item, crate::content::catalog())
}

#[cfg(test)]
pub(super) fn item_color(item: ItemId) -> [f32; 4] {
    item_color_for(item, crate::content::catalog())
}

pub(super) fn item_name_for(item: ItemId, catalog: &Catalog) -> &str {
    catalog
        .item(item)
        .map_or("UNKNOWN", |definition| definition.name.as_ref())
}

pub(super) fn item_color_for(item: ItemId, catalog: &Catalog) -> [f32; 4] {
    catalog
        .item(item)
        .map_or([0.6, 0.3, 0.8, 1.0], |definition| definition.swatch)
}

fn text_width(text: &str, scale: f32, font_scale: f32) -> f32 {
    text.chars().count().min(128) as f32 * 18.0 * scale * font_scale
}

const TEXT: [f32; 4] = [0.90, 0.94, 0.87, 1.0];
const MUTED: [f32; 4] = [0.61, 0.68, 0.63, 1.0];
const GOLD: [f32; 4] = [0.89, 0.72, 0.34, 1.0];
const EDGE: [f32; 4] = [0.28, 0.38, 0.32, 0.93];

pub(super) const UI_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) textured: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) textured: f32,
};

@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    output.textured = input.textured;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    if input.textured > 0.5 {
        return input.color * textureSample(font_atlas, font_sampler, input.uv);
    }
    return input.color;
}
"#;
