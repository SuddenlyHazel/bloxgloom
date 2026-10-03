//! Native command discovery and creative inventory controls.
use super::*;
use bloxgloom_host_api::actions::CommandArgument;

pub(super) fn draw(
    ui: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    intents: &mut Vec<Intent>,
) {
    title(ui, "Commands", "Registered commands and creative inventory");
    let bindings = frame.admin_input.starts_with(UiFrame::BINDING_VIEW_PREFIX);
    if ui
        .button(if bindings { "Commands" } else { "Bind keys" })
        .clicked()
    {
        intents.push(Intent::Control(UiControl::AdminBindings));
    }
    ui.add_space(6.0);
    if frame.admin_enabled {
        let mut flying = frame.flying;
        if ui
            .add_enabled(
                !frame.flying_pending,
                egui::Checkbox::new(&mut flying, "Flying"),
            )
            .changed()
        {
            intents.push(Intent::Control(UiControl::AdminFlying));
        }
        ui.label(
            RichText::new(if frame.flying_pending {
                "Waiting for server…"
            } else if frame.flying {
                "Space rises · Ctrl descends"
            } else {
                "Walking · Space jumps · Shift crouches"
            })
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(6.0);
    }
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
        ui.label(
            RichText::new("Available commands · click to fill the command field")
                .size(11.0)
                .color(MUTED),
        );
        egui::ScrollArea::vertical()
            .id_salt("registered-command-list")
            .max_height(118.0)
            .show(ui, |ui| {
                for action in catalog.registered_actions().filter(|a| a.command.is_some()) {
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
                    if ui
                        .button(RichText::new(signature).monospace().size(11.0))
                        .clicked()
                    {
                        intents.push(Intent::AdminInput(format!("{} ", action.key)));
                    }
                }
            });
        ui.add_space(6.0);
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
