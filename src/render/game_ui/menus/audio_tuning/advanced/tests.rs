use super::*;
fn label(shapes: &[egui::epaint::ClippedShape], name: &str) -> egui::Pos2 {
    fn find(s: &egui::epaint::Shape, name: &str) -> Option<egui::Pos2> {
        match s {
            egui::epaint::Shape::Text(t) if t.galley.job.text == name => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(s) => s.iter().find_map(|s| find(s, name)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|s| find(&s.shape, name))
        .unwrap_or_else(|| panic!("Missing {name}"))
}
fn click(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
#[test]
fn native_species_selector_audition_and_thunder_button_are_wired() {
    let context = crate::render::game_ui::themed_context();
    let mut config = Advanced::default();
    let mut run = |events: Vec<egui::Event>| {
        let mut intents = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| super::draw(ui, &mut config, 0, &mut intents),
        );
        output.textures_delta.clear();
        (output, intents)
    };
    run(vec![]);
    let output = run(vec![]).0;
    let pos = label(&output.shapes, "Cicadas");
    run(click(pos, true));
    run(click(pos, false));
    for _ in 0..12 {
        run(vec![]);
    }
    let output = run(vec![]).0;
    let pos = label(&output.shapes, "Dog-day (US)");
    run(click(pos, true));
    run(click(pos, false));
    let output = run(vec![]).0;
    let hover = label(&output.shapes, "Minminzemi (JP)");
    run(vec![
        egui::Event::PointerMoved(hover),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -80.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    for _ in 0..16 {
        run(vec![]);
    }
    let output = run(vec![]).0;
    let pos = label(&output.shapes, "Pharaoh (US)");
    run(click(pos, true));
    run(click(pos, false));
    let output = run(vec![]).0;
    label(&output.shapes, "Pharaoh (US)");
    let pos = label(&output.shapes, "Audition cicadas");
    run(click(pos, true));
    let (_, intents) = run(click(pos, false));
    assert!(matches!(&intents[..], [Intent::AudioPreview(1)]));
    let output = run(vec![]).0;
    let pos = label(&output.shapes, "Thunder");
    run(click(pos, true));
    run(click(pos, false));
    for _ in 0..12 {
        run(vec![]);
    }
    let output = run(vec![]).0;
    let pos = label(&output.shapes, "Trigger thunder");
    run(click(pos, true));
    let (_, intents) = run(click(pos, false));
    assert!(
        matches!(&intents[..],[Intent::AudioThunder { distance,angle }] if *distance==1200.0 && *angle==0.7)
    );
    assert_eq!(config.cicadas.species, CicadaSpecies::Pharaoh);
    assert_eq!(config.cicadas.tone.pitch_hz, 1400.0);
    assert!(config.preview.manual);
    assert_eq!(config.preview.fixed.daylight, 1.0);
    assert_eq!(config.preview.fixed.rain_mm_h, 0.0);
}
