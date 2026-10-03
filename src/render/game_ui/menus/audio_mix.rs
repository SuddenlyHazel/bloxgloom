//! Local bus controls. Packages route voices; the listener owns global mixing.
use super::*;
use crate::audio::mix_tuning::{CompressorConfig, MixConfig};
pub(super) fn draw(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let mut config = frame.settings.audio_mix;
    egui::CollapsingHeader::new("Mixer buses and compression").show(ui, |ui| {
        ui.label(RichText::new("Changes apply live and save locally. Compression is off by default. Ambient and Music follow Ambient volume; Effects and UI follow Effects volume. Master mutes everything.").small().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Copy mixer settings").clicked() { ui.ctx().copy_text(config.export()); }
            if ui.button("Reset mixer defaults").clicked() { config = MixConfig::default(); }
        });
        for (index, label) in ["Ambient bus", "Effects bus", "UI bus", "Music bus"].into_iter().enumerate() {
            egui::CollapsingHeader::new(label).id_salt(("mixer-bus", index)).show(ui, |ui| {
                let bus = &mut config.buses[index];
                ui.add(egui::Slider::new(&mut bus.gain, 0.0..=4.0).text("Bus volume").suffix("×"));
                compressor(ui, &mut bus.compressor);
            });
        }
        egui::CollapsingHeader::new("Master compressor").show(ui, |ui| compressor(ui, &mut config.master));
        ui.label(RichText::new("Stereo-linked compression keeps left/right balance. The safety limiter stays enabled. Bus volume 0 silences its sources.").small().color(MUTED));
    });
    if config != frame.settings.audio_mix {
        intents.push(Intent::AudioMix(config.sanitized()));
    }
}
fn compressor(ui: &mut egui::Ui, config: &mut CompressorConfig) {
    ui.checkbox(&mut config.enabled, "Enable compressor");
    ui.add_enabled_ui(config.enabled, |ui| {
        ui.add(
            egui::Slider::new(&mut config.threshold_db, -60.0..=0.0)
                .text("Threshold")
                .suffix(" dB"),
        );
        ui.add(
            egui::Slider::new(&mut config.ratio, 1.0..=20.0)
                .text("Ratio")
                .suffix(":1"),
        );
        ui.add(
            egui::Slider::new(&mut config.attack_ms, 0.1..=200.0)
                .logarithmic(true)
                .text("Attack")
                .suffix(" ms"),
        );
        ui.add(
            egui::Slider::new(&mut config.release_ms, 5.0..=2000.0)
                .logarithmic(true)
                .text("Release")
                .suffix(" ms"),
        );
        ui.add(
            egui::Slider::new(&mut config.knee_db, 0.0..=24.0)
                .text("Knee")
                .suffix(" dB"),
        );
        ui.add(
            egui::Slider::new(&mut config.makeup_db, 0.0..=24.0)
                .text("Makeup gain")
                .suffix(" dB"),
        );
    });
}
