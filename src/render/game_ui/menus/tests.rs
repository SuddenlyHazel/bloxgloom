use super::*;

#[test]
fn graphics_settings_do_not_offer_a_model_override() {
    let rows = settings_rows(crate::ui::UiSettings::default(), true);
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.1 != "Characters"));
}

fn label_center(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
        .expect("visible menu control")
}

#[test]
fn native_character_menu_keeps_apply_visible_and_blocks_repeat_while_pending() {
    for size in [egui::vec2(640.0, 360.0), egui::vec2(1280.0, 720.0)] {
        let context = crate::render::game_ui::themed_context();
        let catalog = crate::content::Catalog::builtins();
        let mut frame = UiFrame {
            screen: UiScreen::Character,
            character: Some(crate::ui::CharacterPanel {
                cosmetics: [0; 4],
                recipe: Some(crate::appearance::CharacterRecipe {
                    iris: Some([12, 170, 255]),
                    ..Default::default()
                }),
                can_apply: true,
                pending: false,
                status: "Unapplied changes",
                clip: 0,
                time: 0.0,
                preview: Some(egui::TextureId::User(42)),
            }),
            ..Default::default()
        };
        let draw = |frame: &UiFrame<'_>, events| {
            let mut intents = Vec::new();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| super::draw(ui, frame, &catalog, &mut intents),
            );
            output.textures_delta.clear();
            (output, intents)
        };
        draw(&frame, vec![]);
        let (output, _) = draw(&frame, vec![]);
        label_center(&output.shapes, "Body");
        label_center(&output.shapes, "Hair color");
        let apply = label_center(&output.shapes, "Apply");
        let close = label_center(&output.shapes, "Close");
        assert!(
            apply.y < size.y && close.y < size.y,
            "Apply and Close must fit without scrolling"
        );
        let click = |position, pressed| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        draw(&frame, click(apply, true));
        let (_, intents) = draw(&frame, click(apply, false));
        assert!(
            intents
                .iter()
                .any(|intent| matches!(intent, Intent::Control(UiControl::ApplyCharacter)))
        );
        frame.character.as_mut().unwrap().pending = true;
        draw(&frame, click(apply, true));
        let (_, intents) = draw(&frame, click(apply, false));
        assert!(
            !intents
                .iter()
                .any(|intent| matches!(intent, Intent::Control(UiControl::ApplyCharacter)))
        );
        draw(&frame, click(close, true));
        let (_, intents) = draw(&frame, click(close, false));
        assert!(
            intents
                .iter()
                .any(|intent| matches!(intent, Intent::Control(UiControl::Back)))
        );
    }
}
