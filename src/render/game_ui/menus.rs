//! Built-in menus on the same egui input and paint path as inventory.

use super::Intent;
#[cfg(test)]
mod tests;

mod audio;
mod audio_tuning;
mod character;
mod join;
use crate::{
    content::Catalog,
    ui::{SettingId, UiControl, UiFrame, UiScreen},
};
use bloxgloom_host_api::actions::{CommandArgument, Widget};
use egui::{Align, Color32, Margin, RichText, Stroke, Vec2};

const GOLD: Color32 = Color32::from_rgb(224, 191, 111);
const MUTED: Color32 = Color32::from_rgb(167, 181, 166);
const PANEL: Color32 = Color32::from_rgb(33, 45, 42);
const EDGE: Color32 = Color32::from_rgb(92, 111, 98);

pub(super) fn draw(
    root: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    intents: &mut Vec<Intent>,
) {
    if matches!(frame.screen, UiScreen::Joining | UiScreen::JoinFailed) {
        join::draw(root, frame, intents);
        return;
    }
    let viewport = root.max_rect();
    root.painter()
        .rect_filled(viewport, 0.0, Color32::from_black_alpha(160));
    let compact = viewport.width() < 800.0 || viewport.height() < 500.0;
    let width = (viewport.width() - if compact { 16.0 } else { 80.0 }).min(680.0);
    let height = (viewport.height() - if compact { 12.0 } else { 80.0 }).min(610.0);
    let card = egui::Rect::from_center_size(viewport.center(), Vec2::new(width, height));
    root.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(2.0, EDGE))
            .corner_radius(egui::CornerRadius::same(11))
            .inner_margin(Margin::same(if compact { 12 } else { 22 }))
            .show(ui, |ui| {
                ui.set_width(width - if compact { 24.0 } else { 44.0 });
                // Compact menus can scroll. Reserve a gutter so the scrollbar
                // cannot intercept clicks on the right-aligned setting buttons.
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                egui::ScrollArea::vertical()
                    .id_salt("built-in-menu-scroll")
                    .max_height(height - if compact { 24.0 } else { 44.0 })
                    .show(ui, |ui| match frame.screen {
                        UiScreen::Pause => pause(ui, intents),
                        UiScreen::Audio => audio::draw(ui, frame, intents),
                        UiScreen::Character => character::draw(ui, frame, intents),
                        UiScreen::Settings | UiScreen::Graphics => settings(ui, frame, intents),
                        UiScreen::Actions => actions(ui, frame, intents),
                        UiScreen::Admin => admin(ui, frame, catalog, intents),
                        _ => {}
                    });
            });
    });
}

fn title(ui: &mut egui::Ui, name: &str, detail: &str) {
    ui.label(
        RichText::new("BLOXGLOOM")
            .monospace()
            .size(11.0)
            .color(GOLD),
    );
    ui.heading(name);
    if !detail.is_empty() {
        ui.label(RichText::new(detail).size(12.0).color(MUTED));
    }
    ui.separator();
    ui.add_space(6.0);
}

fn button(ui: &mut egui::Ui, label: &str, control: UiControl, intents: &mut Vec<Intent>) {
    if ui
        .add_sized([ui.available_width(), 35.0], egui::Button::new(label))
        .clicked()
    {
        intents.push(Intent::Control(control));
    }
}

fn pause(ui: &mut egui::Ui, intents: &mut Vec<Intent>) {
    title(ui, "Paused", "Escape returns to the world");
    for (label, control) in [
        ("Resume", UiControl::Resume),
        ("Settings", UiControl::OpenSettings),
        ("Character", UiControl::OpenCharacter),
        ("Commands", UiControl::OpenAdmin),
        ("Exit game", UiControl::Exit),
    ] {
        button(ui, label, control, intents);
        ui.add_space(6.0);
    }
}

fn settings(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let graphics = frame.screen == UiScreen::Graphics;
    title(
        ui,
        if graphics { "Graphics" } else { "Settings" },
        "Local client options",
    );
    if !graphics {
        button(ui, "Audio", UiControl::OpenAudio, intents);
        ui.add_space(6.0);
    }

    if ui
        .button(if graphics { "General" } else { "Graphics" })
        .clicked()
    {
        intents.push(Intent::Control(UiControl::ToggleSettingsPage));
    }
    ui.add_space(8.0);
    let settings = frame.settings;
    let rows = settings_rows(settings, graphics);
    for (setting, label, value) in rows {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.button("+").clicked() {
                    intents.push(Intent::Control(UiControl::Increase(setting)));
                }
                ui.label(RichText::new(value).monospace().color(GOLD));
                if ui.button("−").clicked() {
                    intents.push(Intent::Control(UiControl::Decrease(setting)));
                }
            });
        });
        ui.separator();
    }
    ui.add_space(5.0);
    button(
        ui,
        if settings.fullscreen {
            "Fullscreen: on"
        } else {
            "Fullscreen: off"
        },
        UiControl::ToggleFullscreen,
        intents,
    );
    ui.add_space(6.0);
    button(ui, "Back", UiControl::Back, intents);
}

fn settings_rows(
    settings: crate::ui::UiSettings,
    graphics: bool,
) -> Vec<(SettingId, &'static str, String)> {
    if graphics {
        vec![
            (
                SettingId::PostProcessing,
                "Post effects",
                if settings.post_processing {
                    "On"
                } else {
                    "Off"
                }
                .into(),
            ),
            (
                SettingId::Exposure,
                "Exposure",
                format!("{:.2}×", settings.exposure),
            ),
            (
                SettingId::Bloom,
                "Bloom",
                if settings.bloom_enabled { "On" } else { "Off" }.into(),
            ),
            (
                SettingId::BloomStrength,
                "Bloom strength",
                format!("{:.0}%", settings.bloom_strength * 100.0),
            ),
            (
                SettingId::SunShadows,
                "Sun shadows",
                settings.sun_shadow_quality.label().into(),
            ),
            (
                SettingId::LodHorizon,
                "Distant terrain",
                if settings.lod_horizon == 0 {
                    "Off".into()
                } else {
                    format!("{} blocks", settings.lod_horizon)
                },
            ),
            (
                SettingId::LodQuality,
                "Distant detail",
                ["Coarse", "Balanced", "Detailed"][usize::from(settings.lod_quality.min(2))].into(),
            ),
            (
                SettingId::Parallax,
                "Parallax",
                if settings.parallax.enabled {
                    "On"
                } else {
                    "Off"
                }
                .into(),
            ),
            (
                SettingId::ParallaxDepth,
                "Parallax depth",
                format!("{:.1}%", settings.parallax.depth * 100.0),
            ),
            (
                SettingId::ParallaxDistance,
                "Parallax distance",
                format!("{:.0} blocks", settings.parallax.distance),
            ),
            (
                SettingId::ParallaxQuality,
                "Parallax quality",
                format!("{} samples", settings.parallax.steps),
            ),
        ]
    } else {
        vec![
            (
                SettingId::Sensitivity,
                "Mouse sensitivity",
                format!("{:.3}", settings.sensitivity),
            ),
            (
                SettingId::FieldOfView,
                "Field of view",
                format!("{:.0}°", settings.fov_degrees),
            ),
            (
                SettingId::ViewDistance,
                "View distance",
                settings.view_distance.to_string(),
            ),
            (
                SettingId::UiScale,
                "UI scale",
                format!("{:.1}×", settings.scale),
            ),
            (
                SettingId::Lighting,
                "Lighting",
                if settings.bounced_gi {
                    "Bounced"
                } else {
                    "Voxel"
                }
                .into(),
            ),
        ]
    }
}

fn actions(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let Some(panel) = frame
        .action_panel
        .as_ref()
        .filter(|panel| panel.validate().is_ok())
    else {
        return;
    };
    title(ui, &panel.title, "Server-authorized actions");
    for (index, widget) in panel.widgets.iter().enumerate() {
        match widget {
            Widget::Label(text) => {
                ui.label(text);
            }
            Widget::Button { label, tooltip, .. } => {
                let response = ui.add_sized([ui.available_width(), 35.0], egui::Button::new(label));
                if response.clone().on_hover_text(tooltip).clicked() {
                    intents.push(Intent::Control(UiControl::Action(index as u8)));
                }
            }
        }
        ui.add_space(6.0);
    }
    ui.label(
        RichText::new("Escape closes this panel")
            .size(11.0)
            .color(MUTED),
    );
}

fn admin(ui: &mut egui::Ui, frame: &UiFrame<'_>, catalog: &Catalog, intents: &mut Vec<Intent>) {
    title(ui, "Commands", "Registered commands and creative inventory");
    let bindings = frame.admin_input.starts_with(UiFrame::BINDING_VIEW_PREFIX);
    if ui
        .button(if bindings { "Commands" } else { "Bind keys" })
        .clicked()
    {
        intents.push(Intent::Control(UiControl::AdminBindings));
    }
    ui.add_space(6.0);
    if bindings {
        ui.label(
            RichText::new("Choose a row, then press a free letter. Escape cancels.")
                .size(11.0)
                .color(MUTED),
        );
        for (index, line) in frame.admin_input.split('\n').skip(1).take(8).enumerate() {
            button(ui, line, UiControl::AdminBindingRow(index as u8), intents);
        }
    } else {
        if frame.admin_enabled {
            ui.label(
                RichText::new("Grant a stack of the selected item")
                    .size(11.0)
                    .color(MUTED),
            );
            egui::Grid::new("creative-items")
                .num_columns(4)
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    for (index, item) in catalog
                        .items()
                        .skip(frame.admin_page * 24)
                        .take(24)
                        .enumerate()
                    {
                        if ui
                            .add_sized([110.0, 29.0], egui::Button::new(item.name.as_ref()))
                            .clicked()
                        {
                            intents.push(Intent::Control(UiControl::AdminItem(index as u8)));
                        }
                        if index % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
        } else {
            ui.label(RichText::new("Available commands").size(11.0).color(MUTED));
            for action in catalog
                .registered_actions()
                .filter(|a| a.command.is_some())
                .skip(frame.admin_page * 8)
                .take(8)
            {
                let mut signature = action.key.clone();
                let command = action.command.as_ref().unwrap();
                if !command.aliases.is_empty() {
                    signature.push_str(&format!(" ({})", command.aliases.join(", ")));
                }
                for argument in &action.command.as_ref().unwrap().arguments {
                    signature.push_str(match argument {
                        CommandArgument::Text { .. } => " <text>",
                        CommandArgument::Integer { .. } => " <integer>",
                        CommandArgument::Number { .. } => " <number>",
                        CommandArgument::Player => " <player>",
                        CommandArgument::ItemKey { .. } => " <item>",
                        CommandArgument::EntityKey { .. } => " <entity>",
                        CommandArgument::Count { default: Some(_) } => " [count]",
                        CommandArgument::Count { default: None } => " <count>",
                    });
                }
                ui.label(RichText::new(signature).monospace().size(11.0));
            }
        }
        ui.add_space(6.0);
        ui.label("Command");
        let mut input = frame.admin_input.to_owned();
        let response = ui.add_sized(
            [ui.available_width(), 30.0],
            egui::TextEdit::singleline(&mut input).char_limit(1024),
        );
        if response.changed() {
            intents.push(Intent::AdminInput(input));
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            intents.push(Intent::Control(UiControl::AdminRun));
        }
        button(ui, "Run", UiControl::AdminRun, intents);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Previous").clicked() {
            intents.push(Intent::Control(UiControl::AdminPrev));
        }
        ui.label(
            RichText::new(format!("Page {}", frame.admin_page + 1))
                .monospace()
                .color(GOLD),
        );
        if ui.button("Next").clicked() {
            intents.push(Intent::Control(UiControl::AdminNext));
        }
    });
    if let Some(status) = frame.status {
        ui.separator();
        ui.label(RichText::new(status).color(GOLD));
    }
}
