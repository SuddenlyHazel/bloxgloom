use super::*;

fn text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn collect(shape: &egui::Shape, output: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => output.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, output)),
            _ => {}
        }
    }
    let mut output = Vec::new();
    for shape in shapes {
        collect(&shape.shape, &mut output);
    }
    output
}

#[test]
fn f3_hud_shows_position_and_negative_chunk_coordinates_only_when_enabled() {
    for size in [egui::vec2(640.0, 360.0), egui::vec2(1280.0, 720.0)] {
        let context = crate::render::game_ui::themed_context();
        let catalog = Catalog::builtins();
        for enabled in [false, true] {
            let frame = UiFrame {
                debug: enabled.then_some(crate::ui::UiDebug {
                    position: [-0.1, 80.5, -16.1],
                    ..Default::default()
                }),
                ..Default::default()
            };
            // A warm frame permits egui to finish layout/font setup.
            for _ in 0..2 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| super::draw(ui, &frame, &catalog),
                );
                output.textures_delta.clear();
                let labels = text(&output.shapes);
                assert_eq!(
                    labels.iter().any(|s| s == "XYZ: -0.1 / 80.5 / -16.1"),
                    enabled
                );
                assert_eq!(labels.iter().any(|s| s == "Chunk: -1 / 5 / -2"), enabled);
            }
        }
    }
}
