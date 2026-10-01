//! Native local sound controls. Preview selection is never saved as world state.
use super::*;
pub(super) fn draw(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    title(ui, "Audio", "Local sound settings");
    for (id, label, value) in [
        (
            SettingId::AudioMaster,
            "Master",
            format!("{:.0}%", frame.settings.audio_master * 100.0),
        ),
        (
            SettingId::AudioAmbient,
            "Ambient",
            format!("{:.0}%", frame.settings.audio_ambient * 100.0),
        ),
        (
            SettingId::AudioEffects,
            "Effects",
            format!("{:.0}%", frame.settings.audio_effects * 100.0),
        ),
        (
            SettingId::AudioPreview,
            "Local preview",
            crate::audio::Preset::from_index(frame.settings.audio_preset)
                .label()
                .into(),
        ),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("+").clicked() {
                    intents.push(Intent::Control(UiControl::Increase(id)));
                }
                ui.label(value);
                if ui.small_button("−").clicked() {
                    intents.push(Intent::Control(UiControl::Decrease(id)));
                }
            });
        });
        ui.separator();
    }
    ui.label(
        RichText::new("Preview plays only on this client. It does not change world weather.")
            .color(MUTED),
    );
    ui.add_space(8.0);
    button(ui, "Test sound", UiControl::AudioTest, intents);
    ui.add_space(6.0);
    button(ui, "Back", UiControl::Back, intents);
}
