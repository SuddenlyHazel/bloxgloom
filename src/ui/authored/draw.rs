use super::super::{UiRect, draw::UiBuilder};
use super::*;

impl Session {
    pub(in crate::ui) fn draw_chrome(&self, builder: &mut UiBuilder<'_>) {
        builder.rect(
            0.0,
            0.0,
            builder.width,
            builder.height,
            [0.015, 0.025, 0.035, 0.94],
        );
        builder.text(
            &format!(
                "PACKAGE UI: {}  /  ESC CLOSE  /  PAGE DOWN NEXT",
                self.document().id
            ),
            12.0,
            8.0,
            0.6,
            [0.9, 0.85, 0.6, 1.0],
            96,
        );
        if let Some(id) = self.focused_id() {
            let label = self.event().map_or_else(
                || id.to_owned(),
                |event| format!("{id}  /  {event} ({})", self.event_status()),
            );
            builder.text(
                &label,
                12.0,
                builder.height - 24.0,
                0.6,
                [0.9, 0.85, 0.6, 1.0],
                96,
            );
        }
    }

    pub(in crate::ui) fn draw(&self, builder: &mut UiBuilder<'_>) {
        for (index, node) in self.document().nodes.iter().enumerate() {
            if !self.is_visible(index) {
                continue;
            }
            let Some(&rect) = self.rects.get(index) else {
                continue;
            };
            let clip = self.clips[index];
            let color = linear_color(node.style.color);
            builder.rect(
                clip.x,
                clip.y,
                clip.width,
                clip.height,
                linear_color(node.style.background),
            );
            if self.focused == Some(index) {
                builder.rect(
                    clip.x,
                    clip.y,
                    clip.width,
                    2.0_f32.min(clip.height),
                    [0.95, 0.78, 0.3, 1.0],
                );
            }
            if let Some(image) = &node.image {
                quad(builder, rect, clip, self.resources.images[image], [1.0; 4]);
            }
            if matches!(node.kind, Kind::Label | Kind::Button | Kind::Input) {
                let font = &self.resources.fonts[node.style.font.as_ref().unwrap()];
                let text = self.text_at(index);
                let padding = f32::from(node.style.padding) * self.scale;
                let mut x = rect.x + padding;
                for byte in text.bytes() {
                    let glyph = &font[(byte - b' ') as usize];
                    quad(
                        builder,
                        UiRect {
                            x: x + glyph.offset[0] * self.scale,
                            y: rect.y + padding + glyph.offset[1] * self.scale,
                            width: glyph.uv.width * self.scale,
                            height: glyph.uv.height * self.scale,
                        },
                        clip,
                        glyph.uv,
                        color,
                    );
                    x += glyph.advance * self.scale;
                }
                if node.kind == Kind::Input && self.focused == Some(index) {
                    let caret = session::intersect(
                        UiRect {
                            x,
                            y: rect.y + padding + 3.0 * self.scale,
                            width: self.scale,
                            height: 22.0 * self.scale,
                        },
                        clip,
                    );
                    builder.rect(caret.x, caret.y, caret.width, caret.height, color);
                }
            }
        }
    }
}

fn quad(builder: &mut UiBuilder<'_>, rect: UiRect, clip: UiRect, uv: UiRect, color: [f32; 4]) {
    let clipped = session::intersect(rect, clip);
    if clipped.width <= 0.0 || clipped.height <= 0.0 {
        return;
    }
    let u = uv.x + (clipped.x - rect.x) / rect.width * uv.width;
    let v = uv.y + (clipped.y - rect.y) / rect.height * uv.height;
    builder.glyph(
        clipped.x,
        clipped.y,
        clipped.width,
        clipped.height,
        [u / ATLAS_SIZE as f32, v / ATLAS_SIZE as f32],
        [
            (u + clipped.width / rect.width * uv.width) / ATLAS_SIZE as f32,
            (v + clipped.height / rect.height * uv.height) / ATLAS_SIZE as f32,
        ],
        color,
    );
}

fn linear_color(color: [u8; 4]) -> [f32; 4] {
    let mut result = color.map(|c| f32::from(c) / 255.0);
    for channel in &mut result[..3] {
        *channel = if *channel <= 0.04045 {
            *channel / 12.92
        } else {
            ((*channel + 0.055) / 1.055).powf(2.4)
        };
    }
    result
}
