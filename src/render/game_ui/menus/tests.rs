use super::*;

#[test]
fn admin_flying_checkbox_clicks_and_disables_while_waiting_for_server() {
    for size in [egui::vec2(640.0, 360.0), egui::vec2(1280.0, 720.0)] {
        let context = crate::render::game_ui::themed_context();
        let catalog = crate::content::Catalog::builtins();
        for pending in [false, true] {
            let frame = UiFrame {
                screen: UiScreen::Admin,
                admin_enabled: true,
                flying_pending: pending,
                ..Default::default()
            };
            let draw = |events| {
                let mut intents = Vec::new();
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        ..Default::default()
                    },
                    |ui| super::draw(ui, &frame, &catalog, &mut intents),
                );
                output.textures_delta.clear();
                (output, intents)
            };
            draw(vec![]);
            let (output, _) = draw(vec![]);
            let pos = label_center(&output.shapes, "Flying");
            assert!(pos.y > 0.0 && pos.y < size.y);
            let click = |pressed| {
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            draw(click(true));
            let (_, intents) = draw(click(false));
            if pending {
                assert!(intents.is_empty());
            } else {
                assert!(matches!(
                    intents.as_slice(),
                    [Intent::Control(UiControl::AdminFlying)]
                ));
            }
        }
    }
}

#[test]
fn graphics_settings_do_not_offer_a_model_override() {
    let rows = settings_rows(crate::ui::UiSettings::default(), true);
    assert_eq!(rows.len(), 11);
    assert!(rows.iter().all(|row| row.1 != "Characters"));
}

#[test]
fn native_graphics_lod_values_cover_off_horizons_and_quality_choices() {
    for (horizon, quality, expected_horizon, expected_quality) in [
        (0, 0, "Off", "Coarse"),
        (512, 1, "512 blocks", "Balanced"),
        (1024, 2, "1024 blocks", "Detailed"),
    ] {
        let rows = settings_rows(
            crate::ui::UiSettings {
                lod_horizon: horizon,
                lod_quality: quality,
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.0 == SettingId::LodHorizon)
                .unwrap()
                .2,
            expected_horizon
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.0 == SettingId::LodQuality)
                .unwrap()
                .2,
            expected_quality
        );
    }
}

#[test]
fn native_graphics_rows_show_each_sun_shadow_quality_without_replacing_post_controls() {
    use crate::config::SunShadowQuality;
    for quality in [
        SunShadowQuality::Off,
        SunShadowQuality::Low,
        SunShadowQuality::Medium,
        SunShadowQuality::High,
    ] {
        let settings = crate::ui::UiSettings {
            sun_shadow_quality: quality,
            post_processing: false,
            ..Default::default()
        };
        let rows = settings_rows(settings, true);
        assert_eq!(
            rows.iter().map(|row| row.0).collect::<Vec<_>>(),
            [
                SettingId::PostProcessing,
                SettingId::Exposure,
                SettingId::Bloom,
                SettingId::BloomStrength,
                SettingId::SunShadows,
                SettingId::LodHorizon,
                SettingId::LodQuality,
                SettingId::Parallax,
                SettingId::ParallaxDepth,
                SettingId::ParallaxDistance,
                SettingId::ParallaxQuality,
            ]
        );
        assert_eq!(rows[4].1, "Sun shadows");
        assert_eq!(rows[4].2, quality.label());
        assert!(
            !settings_rows(settings, false)
                .iter()
                .any(|row| row.0 == SettingId::SunShadows)
        );
    }
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
        .unwrap_or_else(|| panic!("visible menu control {label}"))
}

#[test]
fn native_graphics_adjusters_dispatch_their_own_controls() {
    fn row_button(shapes: &[egui::epaint::ClippedShape], label: &str, y: f32) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                    let center = text.pos + text.galley.rect.center().to_vec2();
                    ((center.y - y).abs() < 2.0).then_some(center)
                }
                _ => None,
            })
            .expect("sun shadow adjuster in the same row")
    }
    for size in [egui::vec2(640.0, 360.0), egui::vec2(1280.0, 720.0)] {
        let context = crate::render::game_ui::themed_context();
        let catalog = crate::content::Catalog::builtins();
        let frame = UiFrame {
            screen: UiScreen::Graphics,
            ..Default::default()
        };
        let draw = |events| {
            let mut intents = Vec::new();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| super::draw(ui, &frame, &catalog, &mut intents),
            );
            output.textures_delta.clear();
            (output, intents)
        };
        draw(vec![]);
        let (mut output, _) = draw(vec![]);
        label_center(&output.shapes, "Medium");
        for (name, setting) in [
            ("Sun shadows", SettingId::SunShadows),
            ("Distant terrain", SettingId::LodHorizon),
            ("Distant detail", SettingId::LodQuality),
            ("Parallax", SettingId::Parallax),
            ("Parallax depth", SettingId::ParallaxDepth),
            ("Parallax distance", SettingId::ParallaxDistance),
            ("Parallax quality", SettingId::ParallaxQuality),
        ] {
            if size.y < 500.0 && setting == SettingId::LodQuality {
                // Compact native menus intentionally scroll. Exercise that
                // input path before clicking the lower distant-detail row.
                draw(vec![
                    egui::Event::PointerMoved(egui::pos2(size.x * 0.5, size.y * 0.5)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -200.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                for _ in 0..8 {
                    output = draw(vec![]).0;
                }
            }
            let row = label_center(&output.shapes, name);
            assert!(
                row.y > 0.0 && row.y < size.y,
                "{name} must be visible after any required compact-menu scrolling"
            );
            for (label, control) in [
                ("+", UiControl::Increase(setting)),
                ("−", UiControl::Decrease(setting)),
            ] {
                let pos = row_button(&output.shapes, label, row.y);
                let click = |pressed| {
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                draw(click(true));
                let (_, intents) = draw(click(false));
                assert_eq!(intents.len(), 1, "pointer {label} at {pos:?} in {size:?}");
                assert!(matches!(&intents[0], Intent::Control(actual) if *actual == control));
            }
        }
        // Pointer clicks do not request egui keyboard focus. Traverse the real
        // native Tab order and activate each focused widget with Enter instead.
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut keyboard_controls = Vec::new();
        for _ in 0..20 {
            draw(vec![key(egui::Key::Tab, true)]);
            draw(vec![key(egui::Key::Tab, false)]);
            let (_, intents) = draw(vec![key(egui::Key::Enter, true)]);
            keyboard_controls.extend(intents.into_iter().filter_map(|intent| match intent {
                Intent::Control(control) => Some(control),
                _ => None,
            }));
            draw(vec![key(egui::Key::Enter, false)]);
        }
        assert!(keyboard_controls.contains(&UiControl::Increase(SettingId::SunShadows)));
        assert!(keyboard_controls.contains(&UiControl::Decrease(SettingId::SunShadows)));
        assert!(keyboard_controls.contains(&UiControl::Increase(SettingId::LodHorizon)));
        assert!(keyboard_controls.contains(&UiControl::Decrease(SettingId::LodQuality)));
    }
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
        assert!(
            output.shapes.iter().all(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) =>
                    !["Eyes", "Mouth"].contains(&text.galley.job.text.as_str()),
                _ => true,
            }),
            "face selectors must come from the installed GLB"
        );
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

#[test]
fn rain_audio_copy_exports_current_tuning_and_reset_dispatches_live_defaults() {
    for size in [egui::vec2(1280.0, 720.0), egui::vec2(640.0, 360.0)] {
        let context = crate::render::game_ui::themed_context();
        let catalog = crate::content::Catalog::builtins();
        let mut frame = UiFrame {
            screen: UiScreen::Audio,
            ..Default::default()
        };
        frame.settings.rain_audio.bed_gain = 0.037;
        frame.settings.rain_audio.advanced.cicadas.species =
            crate::audio::rain_tuning::CicadaSpecies::Higurashi;
        frame
            .settings
            .rain_audio
            .advanced
            .preview
            .climate
            .gust_intensity = 0.8;
        frame.settings.rain_audio.surfaces[9].modes[0].frequency_hz = 470.0;
        let draw = |events| {
            let mut intents = Vec::new();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| super::draw(ui, &frame, &catalog, &mut intents),
            );
            output.textures_delta.clear();
            (output, intents)
        };
        draw(vec![]);
        let (mut output, _) = draw(vec![]);
        if size.y < 500.0 {
            draw(vec![
                egui::Event::PointerMoved(egui::pos2(size.x / 2.0, size.y / 2.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -180.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            for _ in 0..8 {
                output = draw(vec![]).0;
            }
        }
        let click = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let pos = label_center(&output.shapes, "Copy audio settings");
        assert!(pos.y > 20.0 && pos.y < size.y - 20.0);
        draw(click(pos, true));
        let (output, intents) = draw(click(pos, false));
        assert!(intents.is_empty(), "copy must not change tuning");
        let copied = output
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .expect("native clipboard copy output");
        assert_eq!(
            *copied,
            frame.settings.rain_audio.export(
                frame.settings.audio_master,
                frame.settings.audio_ambient,
                frame.settings.audio_effects
            )
        );
        label_center(&output.shapes, "Copied!");
        let pos = label_center(&output.shapes, "Reset tuning defaults");
        draw(click(pos, true));
        let (_, intents) = draw(click(pos, false));
        assert!(
            matches!(&intents[..], [Intent::RainAudio(profile)] if **profile == Default::default())
        );
        let output = draw(vec![]).0;
        let pos = label_center(&output.shapes, "Mute rain / wind / insects");
        draw(click(pos, true));
        let (_, intents) = draw(click(pos, false));
        let mut muted = frame.settings.rain_audio;
        muted.mute_ambient();
        assert!(matches!(&intents[..], [Intent::RainAudio(profile)] if **profile == muted));
    }
}
