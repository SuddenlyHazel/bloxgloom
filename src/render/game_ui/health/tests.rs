use super::*;
fn label_rect(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    shapes.iter().find_map(|shape| find(&shape.shape, label))
}
#[test]
fn player_health_live_hud_and_respawn_click_cover_compact_and_large_viewports() {
    for size in [Vec2::new(640., 360.), Vec2::new(1280., 720.)] {
        let context = crate::render::game_ui::themed_context();
        let health = bloxgloom_host_api::player_health::View {
            current: 0,
            max: 100,
            alive: false,
            revision: 1,
            life: 2,
        };
        let frame = UiFrame {
            health: Some(health),
            ..Default::default()
        };
        let draw = |events: Vec<egui::Event>, dead| {
            let mut intents = Vec::new();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if dead {
                        death(ui, &frame, &mut intents)
                    } else {
                        hud(ui, &frame)
                    }
                },
            );
            output.textures_delta.clear();
            (output, intents)
        };
        let (output, _) = draw(vec![], false);
        assert!(label_rect(&output.shapes, "HEALTH 0 / 100").is_some());
        draw(vec![], true);
        let (output, _) = draw(vec![], true);
        let rect = label_rect(&output.shapes, "RESPAWN").expect("live respawn button missing");
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
        let click = |pressed| {
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        draw(click(true), true);
        let (_, intents) = draw(click(false), true);
        assert!(matches!(
            intents.as_slice(),
            [Intent::Control(UiControl::Respawn)]
        ));
    }
}
