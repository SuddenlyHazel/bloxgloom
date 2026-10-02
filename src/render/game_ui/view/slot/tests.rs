use super::*;

#[test]
fn item_visuals_egui_slots_paint_the_worker_selected_stack_icon() {
    let (_bundle, catalog, mut stack) = crate::client::item_visuals::tests::example();
    stack.count = 128;
    crate::client::item_visuals::tests::wait(&catalog, &stack);
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(120.))),
            ..Default::default()
        },
        |ui| {
            paint_icon(
                ui,
                Rect::from_min_size(Pos2::new(20., 20.), Vec2::splat(80.)),
                &stack,
                &catalog,
            );
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(51, 229, 76))), "slot must use the green full-stack callback art");
    // Optional CPU raster of these exact painted rectangles for headless visual QA.
    if let Some(path) = std::env::var_os("BLOXGLOOM_ITEM_ICON_PREVIEW") {
        let mut pixels = [28u8, 37, 34, 255].repeat(120 * 120);
        for shape in &output.shapes {
            if let egui::Shape::Rect(rect) = &shape.shape {
                if rect.fill.a() == 0 {
                    continue;
                }
                let bounds = rect
                    .rect
                    .intersect(Rect::from_min_size(Pos2::ZERO, Vec2::splat(120.)));
                for y in bounds.top().max(0.) as u32..bounds.bottom().min(120.) as u32 {
                    for x in bounds.left().max(0.) as u32..bounds.right().min(120.) as u32 {
                        let offset = ((y * 120 + x) * 4) as usize;
                        pixels[offset..offset + 4].copy_from_slice(&rect.fill.to_array());
                    }
                }
            }
        }
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 120, 120);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
}
