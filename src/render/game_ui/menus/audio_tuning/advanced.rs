//! Remaining live synthesis controls and a local-only weather lab.
use super::*;
use crate::audio::rain_tuning::{Advanced, CicadaSpecies};
macro_rules! controls {
    ($ui:expr,$profile:expr) => {
        $profile.controls(|label, value, range, unit| slider($ui, label, value, range, unit));
    };
}
pub(super) fn draw(
    ui: &mut egui::Ui,
    config: &mut Advanced,
    preview: u8,
    intents: &mut Vec<Intent>,
) {
    egui::CollapsingHeader::new("Wind character").show(ui, |ui| {
        controls!(ui, config.wind);
    });
    egui::CollapsingHeader::new("Crickets").show(ui, |ui| {
        controls!(ui, config.crickets.tone);
        egui::CollapsingHeader::new("Cricket placement").show(ui, |ui| {
            controls!(ui, config.crickets.placement);
            placement_hint(ui);
        });
        if ui.button("Audition crickets").clicked() {
            audition(config, 0.0, intents);
        }
        ui.label(
            RichText::new(
                "World crickets call at night, within these temperature, rain and wind limits.",
            )
            .small()
            .color(MUTED),
        );
    });
    egui::CollapsingHeader::new("Cicadas").show(ui,|ui| {
        let previous=config.cicadas.species;
        egui::ComboBox::from_id_salt("cicada-species").selected_text(previous.label()).show_ui(ui,|ui| {
            for species in CicadaSpecies::ALL { ui.selectable_value(&mut config.cicadas.species,species,species.label()); }
        });
        if previous!=config.cicadas.species { config.cicadas.tone.pitch_hz=config.cicadas.species.pitch(); }
        controls!(ui,config.cicadas.tone);
        egui::CollapsingHeader::new("Cicada placement").show(ui,|ui| { controls!(ui,config.cicadas.placement); placement_hint(ui); });
        if ui.button("Audition cicadas").clicked() { audition(config,1.0,intents); }
        ui.label(RichText::new("World cicadas call during daylight, within these temperature and rain limits. Changing species also selects its natural pitch.").small().color(MUTED));
    });
    egui::CollapsingHeader::new("Thunder").show(ui,|ui| {
        controls!(ui,config.thunder);
        ui.label(RichText::new("Thunder uses Effects. Scatter changes automatic preview strikes; world strikes keep their server positions.").small().color(MUTED));
        if ui.button("Trigger thunder").clicked() { intents.push(Intent::AudioThunder { distance:config.thunder.distance_m,angle:config.thunder.angle_rad }); }
    });
    egui::CollapsingHeader::new("Spatial hearing").show(ui,|ui| {
        controls!(ui,config.listener);
        ui.label(RichText::new("Applies to individual rain impacts and insects. Zero ear spacing centers sources; the rain bed stays diffuse. Sources continue to follow your camera.").small().color(MUTED));
    });
    egui::CollapsingHeader::new("Ambient reverb").show(ui,|ui| {
        controls!(ui,config.reverb);
        ui.label(RichText::new("Damping is the filter response: lower values remove more treble. Set a return to zero to mute its existing tail.").small().color(MUTED));
    });
    egui::CollapsingHeader::new("Local weather preview lab").show(ui,|ui| {
        ui.label(RichText::new("These controls affect audio preview only. Off follows server weather. They do not change world rain, wind or lightning.").color(MUTED));
        if preview==0 {
            if ui.button("Start weather preview").clicked() { intents.push(Intent::AudioPreview(2)); }
        } else if ui.button("Stop preview / follow world").clicked() { intents.push(Intent::AudioPreview(0)); }
        ui.checkbox(&mut config.preview.manual,"Hold custom weather in any active preview");
        egui::CollapsingHeader::new("Held weather").show(ui,|ui| { controls!(ui,config.preview.fixed); });
        egui::CollapsingHeader::new("Storm climate and gusts").show(ui,|ui| {
            controls!(ui,config.preview.climate);
            ui.label(RichText::new("Select Storm preview with custom weather off to hear passing cells. Gust controls also apply to held weather. Frequency changes future arrivals; existing storms finish passing.").small().color(MUTED));
        });
        egui::CollapsingHeader::new("Storm cell shape").show(ui,|ui| { controls!(ui,config.preview.shape); });
    });
}
fn placement_hint(ui: &mut egui::Ui) {
    ui.label(RichText::new("Distance limits gate real habitat sources in the world; preview uses synthetic sources within those limits. Spread affects preview placement only.").small().color(MUTED));
}
fn audition(config: &mut Advanced, daylight: f32, intents: &mut Vec<Intent>) {
    config.preview.manual = true;
    config.preview.fixed.rain_mm_h = 0.0;
    config.preview.fixed.wind_m_s = 0.0;
    config.preview.fixed.lightning_per_min = 0.0;
    config.preview.fixed.daylight = daylight;
    config.preview.fixed.temperature_c = 30.0;
    intents.push(Intent::AudioPreview(1));
}

#[cfg(test)]
mod tests;
