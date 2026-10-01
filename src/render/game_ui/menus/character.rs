//! Local draft editing; only Apply emits a server selection request.
use super::*;
use crate::appearance::CharacterRecipe;

pub(super) fn draw(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
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
    let mut recipe = panel.recipe;
    ui.add_enabled_ui(!panel.pending, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(recipe.is_none(), "Classic").clicked() {
                recipe = None;
            }
            if ui.selectable_label(recipe.is_some(), "Authored").clicked() {
                recipe = Some(recipe.unwrap_or_default());
            }
        });
    });
    let compact = ui
        .ctx()
        .input(|input| input.content_rect().height() < 500.0);
    ui.columns(2, |columns| {
        columns[0].add_enabled_ui(!panel.pending, |ui| {
            if let Some(ref mut value) = recipe {
                choice(ui, "Hair", &crate::appearance::HAIR, &mut value.hair);
                choice(
                    ui,
                    "Eyes",
                    crate::render::character_eye_names(),
                    &mut value.eyes,
                );
                choice(
                    ui,
                    "Mouth",
                    crate::render::character_mouth_names(),
                    &mut value.mouth,
                );
                let mut custom = value.iris.is_some();
                if ui.checkbox(&mut custom, "Custom iris color").changed() {
                    value.iris = custom.then_some(value.iris.unwrap_or([110, 160, 210]));
                }
                if let Some(ref mut rgb) = value.iris {
                    ui.horizontal(|ui| {
                        for (label, channel) in ["R", "G", "B"].into_iter().zip(rgb.iter_mut()) {
                            ui.label(label);
                            ui.add(egui::DragValue::new(channel).range(0..=255));
                        }
                        ui.color_edit_button_srgb(rgb);
                    });
                }
            } else {
                ui.label(
                    RichText::new("Uses the server's existing skin, shirt and pants palettes.")
                        .color(MUTED),
                );
            }
        });
        let mut clip = panel.clip;
        choice(
            &mut columns[0],
            "Animation",
            &["idle", "walk", "crouch", "left tool", "right tool"],
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
    if recipe != panel.recipe && recipe.is_none_or(CharacterRecipe::valid) {
        intents.push(Intent::CharacterRecipe(recipe));
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
