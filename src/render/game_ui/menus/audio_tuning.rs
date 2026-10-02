//! Live rain controls; all values travel through the client's local config worker.
use super::*;
use crate::audio::rain_tuning::{RainConfig, Surface};
use std::ops::RangeInclusive;

pub(super) fn draw(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let mut config = frame.settings.rain_audio;
    ui.heading("Rain tuning");
    ui.label(RichText::new("Changes apply live and save locally. Use the Rain preview to compare materials; select Off to hear world weather.").color(MUTED));
    ui.horizontal_wrapped(|ui| {
        if ui.button("Copy audio settings").clicked() {
            ui.ctx().copy_text(config.export(
                frame.settings.audio_master,
                frame.settings.audio_ambient,
                frame.settings.audio_effects,
            ));
            let time = ui.input(|i| i.time);
            ui.data_mut(|data| data.insert_temp(egui::Id::new("rain-settings-copied"), time));
        }
        if ui.button("Reset rain defaults").clicked() {
            config = RainConfig::default();
        }
        let copied = ui.data(|data| data.get_temp::<f64>(egui::Id::new("rain-settings-copied")));
        if copied.is_some_and(|time| ui.input(|i| i.time) - time < 3.0) {
            ui.label("Copied!");
        }
    });
    slider(ui, "Rain volume", &mut config.gain, 0.0..=4.0, "×");
    slider(ui, "Rain bed", &mut config.bed_gain, 0.0..=4.0, "×");
    slider(
        ui,
        "Individual drops",
        &mut config.drop_gain,
        0.0..=4.0,
        "×",
    );
    slider(ui, "Impact reverb", &mut config.reverb_gain, 0.0..=4.0, "×");
    ui.label(RichText::new("Set Rain bed to 0 to isolate impacts; set Individual drops to 0 to isolate the diffuse bed.").small().color(MUTED));
    egui::CollapsingHeader::new("Arrival, distance and gust controls").show(ui, |ui| {
        slider(ui, "Maximum drop rate", &mut config.max_drops_per_s, 0.0..=2000.0, "/s");
        slider(ui, "Gust modulation", &mut config.sheet_depth, 0.0..=2.0, "×");
        slider(ui, "Near distance", &mut config.min_distance_m, 0.25..=config.max_distance_m, " m");
        slider(ui, "Far distance", &mut config.max_distance_m, config.min_distance_m..=100.0, " m");
        ui.label(RichText::new("Distances control the arrival/bed budget. World impacts keep their actual positions and rotate with your camera.").small().color(MUTED));
    });
    egui::CollapsingHeader::new("Material sound profiles").show(ui, |ui| {
        ui.checkbox(&mut config.use_block_profiles, "Honor custom block sound profiles")
            .on_hover_text("Custom block profiles override timbre. Turn this off to audition these material profiles everywhere. Material volume still applies to custom profiles.");
        for index in 0..config.surfaces.len() {
            let sole_coverage = config.surfaces.iter().enumerate().all(|(other, s)| other == index || s.coverage == 0.0);
            let surface = &mut config.surfaces[index];
            egui::CollapsingHeader::new(surface.name).id_salt(("rain-material", index)).show(ui, |ui| {
                material(ui, surface, sole_coverage);
            });
        }
    });
    if config != frame.settings.rain_audio {
        intents.push(Intent::RainAudio(Box::new(config.sanitized())));
    }
}

fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    suffix: &str,
) {
    let logarithmic = *range.start() > 0.0;
    ui.add(
        egui::Slider::new(value, range)
            .logarithmic(logarithmic)
            .text(label)
            .suffix(suffix),
    );
}
fn pair(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut [f32; 2],
    range: RangeInclusive<f32>,
    suffix: &str,
) {
    let maximum = values[1];
    slider(
        ui,
        &format!("{label} minimum"),
        &mut values[0],
        *range.start()..=maximum,
        suffix,
    );
    let minimum = values[0];
    slider(
        ui,
        &format!("{label} maximum"),
        &mut values[1],
        minimum..=*range.end(),
        suffix,
    );
}
fn material(ui: &mut egui::Ui, surface: &mut Surface, sole_coverage: bool) {
    slider(ui, "Material volume", &mut surface.gain, 0.0..=4.0, "×");
    pair(ui, "Click volume", &mut surface.click_gain, 0.0..=2.0, "×");
    pair(
        ui,
        "Click frequency",
        &mut surface.click_frequency_hz,
        20.0..=20000.0,
        " Hz",
    );
    slider(
        ui,
        "Click damping",
        &mut surface.click_damping_ratio,
        0.05..=50.0,
        "×",
    );
    for (index, mode) in surface.modes.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.label(format!("Resonance {}", index + 1));
            slider(
                ui,
                "Frequency",
                &mut mode.frequency_hz,
                20.0..=20000.0,
                " Hz",
            );
            slider(ui, "Damping", &mut mode.damping_per_s, 1.0..=20000.0, "/s");
            slider(ui, "Volume", &mut mode.gain, 0.0..=4.0, "×");
        });
    }
    slider(ui, "Resonance detune", &mut surface.detune, 0.0..=0.5, "×");
    let mut filter = surface.lowpass_hz != 0.0;
    if ui.checkbox(&mut filter, "Low-pass filter").changed() {
        surface.lowpass_hz = if filter { 4200.0 } else { 0.0 };
    }
    if filter {
        slider(
            ui,
            "Low-pass cutoff",
            &mut surface.lowpass_hz,
            20.0..=20000.0,
            " Hz",
        );
    }
    egui::CollapsingHeader::new("Bubbles / splash detail").show(ui, |ui| {
        slider(
            ui,
            "Bubble probability",
            &mut surface.bubble_probability,
            0.0..=1.0,
            "",
        );
        pair(
            ui,
            "Bubble radius",
            &mut surface.bubble_radius_m,
            0.00016..=0.004,
            " m",
        );
        pair(
            ui,
            "Bubble volume",
            &mut surface.bubble_gain,
            0.0..=8.0,
            "×",
        );
        pair(
            ui,
            "Bubble decay",
            &mut surface.bubble_decay,
            0.25..=20.0,
            "×",
        );
        slider(
            ui,
            "Bubble delay",
            &mut surface.bubble_delay_s,
            0.0..=0.1,
            " s",
        );
    });
    egui::CollapsingHeader::new("Preview material mix").show(ui, |ui| {
        ui.label(RichText::new("Only the local preview uses these weights. World materials come from exposed blocks.").small().color(MUTED));
        slider(ui, "Coverage weight", &mut surface.coverage, (if sole_coverage { 0.0001 } else { 0.0 })..=1.0, "");
        ui.checkbox(&mut surface.vertical, "Wind-facing wall in preview");
    });
}
