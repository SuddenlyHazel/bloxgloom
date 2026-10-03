use super::*;

#[test]
fn controller_pointer_clicks_egui_at_hidpi_scale_and_cancels_cross_screen_drag() {
    let context = egui::Context::default();
    let mut rect = egui::Rect::NOTHING;
    let mut run = |events| {
        let mut clicked = false;
        let mut secondary = false;
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 360.0),
                )),
                ..Default::default()
            },
            |ui| {
                let response = ui.button("Controller selection");
                rect = response.rect;
                clicked = response.clicked();
                secondary = response.secondary_clicked();
                draw_cursor(ui.ctx(), rect.center());
            },
        );
        output.textures_delta.clear();
        (clicked, secondary, rect.center())
    };
    run(vec![]);
    let (_, _, pos) = run(vec![]);
    let mut pointer = Pointer::default();
    for secondary in [false, true] {
        let mut events = Vec::new();
        pointer.append_events(
            &mut events,
            2.0,
            Some([pos.x * 2.0, pos.y * 2.0]),
            Some((true, secondary)),
            [0.0, 0.0],
        );
        run(events);
        let mut events = Vec::new();
        pointer.append_events(&mut events, 2.0, None, Some((false, secondary)), [0.0, 0.0]);
        let (clicked, split, _) = run(events);
        assert_eq!((clicked, split), (!secondary, secondary));
    }
    let mut events = Vec::new();
    pointer.append_events(
        &mut events,
        2.0,
        Some([pos.x * 2.0, pos.y * 2.0]),
        Some((true, false)),
        [0.0, 0.0],
    );
    run(events);
    let mut events = Vec::new();
    pointer.append_events(
        &mut events,
        2.0,
        Some([-1000.0; 2]),
        Some((false, false)),
        [0.0, 0.0],
    );
    assert!(!run(events).0);
    assert!(!pointer.visible);
}
