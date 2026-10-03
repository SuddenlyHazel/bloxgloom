//! Exercise real scrolling and expansion for audio-control screenshots.
fn centre(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, label).filter(|p| s.clip_rect.contains(*p)))
}
pub(super) fn prepare(
    label: &str,
    size: egui::Vec2,
    mut draw: impl FnMut(Vec<egui::Event>) -> egui::FullOutput,
) -> Result<egui::FullOutput, Box<dyn std::error::Error>> {
    let header = match label {
        "audio-mixer" => "Mixer buses and compression",
        "audio-wind" => "Wind character",
        "audio-cicadas" => "Cicadas",
        "audio-thunder" => "Thunder",
        "audio-spatial" => "Spatial hearing",
        "audio-weather" => "Local weather preview lab",
        _ => unreachable!(),
    };
    let mut textures = egui::TexturesDelta::default();
    let mut frame = |events| {
        let mut output = draw(events);
        textures.append(std::mem::take(&mut output.textures_delta));
        output
    };
    let pointer = egui::pos2(size.x * 0.5, size.y * 0.5);
    let wheel = |dy| {
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let mut output = frame(vec![]);
    for _ in 0..3 {
        output = frame(vec![]);
    }
    for _ in 0..30 {
        if centre(&output, header).is_some() {
            break;
        }
        frame(wheel(-80.0));
        for _ in 0..16 {
            output = frame(vec![]);
        }
    }
    let pos =
        centre(&output, header).ok_or_else(|| format!("Audio preview could not reach {header}"))?;
    for pressed in [true, false] {
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    for _ in 0..16 {
        output = frame(vec![]);
    }
    let display_header = if label == "audio-mixer" {
        for _ in 0..30 {
            if centre(&output, "Ambient bus").is_some() {
                break;
            }
            frame(wheel(-60.0));
            for _ in 0..16 {
                output = frame(vec![]);
            }
        }
        let pos =
            centre(&output, "Ambient bus").ok_or("Mixer preview could not reach Ambient bus")?;
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        for _ in 0..16 {
            output = frame(vec![]);
        }
        "Ambient bus"
    } else {
        header
    };
    // Put the expanded section near the top of the scrollable card.
    for _ in 0..3 {
        if let Some(pos) = centre(&output, display_header) {
            frame(wheel(-(pos.y - size.y.min(720.0) * 0.18)));
        }
        for _ in 0..16 {
            output = frame(vec![]);
        }
    }
    output.textures_delta = textures;
    Ok(output)
}
