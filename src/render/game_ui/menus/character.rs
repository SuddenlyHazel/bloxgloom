//! Local draft editing; only Apply emits a server selection request.
use super::*;

pub(super) fn draw(
    ui: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    intents: &mut Vec<Intent>,
) {
    let compact = ui
        .ctx()
        .input(|input| input.content_rect().height() < 500.0);
    if compact {
        // Keep Apply/Close visible even with the optional iris RGB row at 640×360.
        ui.spacing_mut().item_spacing.y = 3.0;
    }
    title(
        ui,
        "Character",
        "Apply saves to this server. Close discards unapplied edits.",
    );
    let Some(panel) = frame.character else {
        ui.label("Waiting for your player snapshot…");
        return;
    };
    ui.label(RichText::new(panel.status).color(MUTED));
    let mut selected = panel.packaged.map(|p| p.model);
    egui::ComboBox::from_label("Model")
        .selected_text(
            selected
                .and_then(|id| catalog.model_key(id))
                .unwrap_or("Builtin character"),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, None, "Builtin character");
            for (id, _) in catalog.models().filter(|(_, m)| m.player.is_some()) {
                ui.selectable_value(&mut selected, Some(id), catalog.model_key(id).unwrap());
            }
        });
    if selected != panel.packaged.map(|p| p.model) {
        intents.push(Intent::CharacterModel(selected));
    }
    let mut recipe = panel.recipe.unwrap_or_default();
    ui.columns(2, |columns| {
        egui::ScrollArea::vertical()
            .id_salt("model controls")
            .max_height(if compact { 110.0 } else { 300.0 })
            .show(&mut columns[0], |ui| {
                ui.add_enabled_ui(!panel.pending, |ui| {
                    if let Some(packaged) = panel.packaged {
                        if let Some(model) = catalog.player_model(packaged.model) {
                            let mut visual = packaged.visual;
                            let schema = model.visual_schema();
                            for (i, (name, options)) in schema.variants.iter().enumerate() {
                                let mut value = visual.variants[i];
                                egui::ComboBox::from_label(name)
                                    .selected_text(if value == 255 {
                                        "Asset default"
                                    } else {
                                        options
                                            .get(value as usize)
                                            .map_or("Unknown", String::as_str)
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut value, 255, "Asset default");
                                        for (id, name) in options.iter().enumerate() {
                                            ui.selectable_value(&mut value, id as u8, name);
                                        }
                                    });
                                visual.variants[i] = value;
                            }
                            for (i, name) in schema.layers.iter().enumerate() {
                                let mut value = visual.layers[i];
                                egui::ComboBox::from_label(name)
                                    .selected_text(match value {
                                        -1 => "Asset default",
                                        0 => "Hidden",
                                        _ => "Visible",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut value, -1, "Asset default");
                                        ui.selectable_value(&mut value, 0, "Hidden");
                                        ui.selectable_value(&mut value, 1, "Visible");
                                    });
                                visual.layers[i] = value;
                            }
                            for (i, name) in schema.tints.iter().enumerate() {
                                let mut enabled = visual.tints[i].is_some();
                                if ui
                                    .checkbox(&mut enabled, format!("Custom {name}"))
                                    .changed()
                                {
                                    visual.tints[i] = enabled.then_some(visual.tints[i].unwrap_or(
                                        bloxgloom_host_api::entity::Tint {
                                            rgb: [255; 3],
                                            mode: bloxgloom_host_api::entity::TintMode::Multiply,
                                        },
                                    ));
                                }
                                if let Some(tint) = &mut visual.tints[i] {
                                    color(ui, name, &mut tint.rgb);
                                    let mut replace =
                                        tint.mode == bloxgloom_host_api::entity::TintMode::Replace;
                                    ui.checkbox(&mut replace, "Replace painted color");
                                    tint.mode = if replace {
                                        bloxgloom_host_api::entity::TintMode::Replace
                                    } else {
                                        bloxgloom_host_api::entity::TintMode::Multiply
                                    };
                                }
                            }
                            if visual != packaged.visual {
                                intents.push(Intent::CharacterModelVisual(visual));
                            }
                        }
                    } else {
                        choice(ui, "Body", &crate::appearance::BODIES, &mut recipe.body);
                        choice(ui, "Hair", &crate::appearance::HAIR, &mut recipe.hair);
                        color(ui, "Hair color", &mut recipe.hair_color);
                        let mut custom = recipe.iris.is_some();
                        if ui.checkbox(&mut custom, "Custom iris color").changed() {
                            recipe.iris = custom.then_some(recipe.iris.unwrap_or([110, 160, 210]));
                        }
                        if let Some(ref mut rgb) = recipe.iris {
                            color(ui, "Iris color", rgb);
                        }
                    }
                });
            });
        let mut clip = panel.clip;
        choice(
            &mut columns[0],
            "Animation",
            &["idle", "walk", "crouch", "left tool", "right tool", "run"],
            &mut clip,
        );
        if clip != panel.clip {
            intents.push(Intent::CharacterClip(clip));
        }
        if let Some(texture) = panel.preview {
            let height = if compact { 155.0 } else { 300.0 };
            columns[1].horizontal(|ui| {
                ui.add_space(((ui.available_width() - height * 2.0 / 3.0) * 0.5).max(0.0));
                ui.image((texture, Vec2::new(height * 2.0 / 3.0, height)));
            });
        }
    });
    if panel.packaged.is_none() && recipe != panel.recipe.unwrap_or_default() && recipe.valid() {
        intents.push(Intent::CharacterRecipe(Some(recipe)));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                panel.can_apply && !panel.pending,
                egui::Button::new("Apply"),
            )
            .clicked()
        {
            intents.push(Intent::Control(UiControl::ApplyCharacter));
        }
        if ui.button("Close").clicked() {
            intents.push(Intent::Control(UiControl::Back));
        }
    });
}

fn choice(ui: &mut egui::Ui, label: &str, names: &[&str], value: &mut u8) {
    egui::ComboBox::from_label(label)
        .selected_text(names[usize::from(*value).min(names.len() - 1)].replace('_', " "))
        .show_ui(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                ui.selectable_value(value, index as u8, name.replace('_', " "));
            }
        });
}

fn color(ui: &mut egui::Ui, label: &str, rgb: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_srgb(rgb);
        for (label, channel) in ["R", "G", "B"].into_iter().zip(rgb.iter_mut()) {
            ui.add(egui::DragValue::new(channel).prefix(label).range(0..=255));
        }
    });
}
