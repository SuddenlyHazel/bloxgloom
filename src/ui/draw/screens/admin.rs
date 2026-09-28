//! Creative catalog and command entry overlay.

use super::*;
use bloxgloom_host_api::actions::{Action, CommandArgument, CommandPermission};

fn command_signature(action: &Action) -> Option<String> {
    let command = action.command.as_ref()?;
    let mut signature = action.key.clone();
    for argument in &command.arguments {
        match argument {
            CommandArgument::ItemKey { .. } => signature.push_str(" <item>"),
            CommandArgument::EntityKey { .. } => signature.push_str(" <entity>"),
            CommandArgument::Count { default: Some(_) } => signature.push_str(" [count]"),
            CommandArgument::Count { default: None } => signature.push_str(" <count>"),
        }
    }
    if command.permission == CommandPermission::Admin {
        signature.push_str("  (ADMIN)");
    }
    Some(signature)
}

impl UiBuilder<'_> {
    pub(in crate::ui::draw) fn draw_admin(
        &mut self,
        frame: &UiFrame<'_>,
        layout: &UiLayout,
        catalog: &crate::content::Catalog,
    ) {
        self.screen_dim();
        let panel = layout.admin_panel();
        self.panel(panel);
        self.text(
            "COMMANDS",
            panel.x + 24.0 * self.scale,
            panel.y + 20.0 * self.scale,
            1.2,
            TEXT,
            30,
        );
        self.text(
            if frame.admin_enabled {
                "CLICK AN ITEM TO GRANT 128  /  F4 OR ESC TO CLOSE"
            } else {
                "ENTER A REGISTERED COMMAND  /  F4 OR ESC TO CLOSE"
            },
            panel.x + 24.0 * self.scale,
            panel.y + 48.0 * self.scale,
            0.62,
            MUTED,
            70,
        );
        for (index, item) in catalog
            .items()
            .skip(frame.admin_page * 24)
            .take(if frame.admin_enabled { 24 } else { 0 })
            .enumerate()
        {
            let control = UiControl::AdminItem(index as u8);
            let Some(rect) = layout.rect(control) else {
                continue;
            };
            self.rounded_panel(
                rect,
                [0.055, 0.075, 0.073, 0.95],
                if frame.hovered == Some(control) {
                    GOLD
                } else {
                    EDGE
                },
                1.0 * self.scale,
            );
            let swatch = UiRect {
                x: rect.x + 6.0 * self.scale,
                y: rect.y + (rect.height - 21.0 * self.scale) * 0.5,
                width: 21.0 * self.scale,
                height: 21.0 * self.scale,
            };
            self.draw_item_swatch(swatch, item.id, catalog);
            let name: String = item.name.chars().take(13).collect();
            self.text(
                &name,
                rect.x + 32.0 * self.scale,
                rect.y + (rect.height - 8.0 * self.scale) * 0.5,
                0.46,
                TEXT,
                13,
            );
        }
        if !frame.admin_enabled {
            for (index, action) in catalog
                .registered_actions()
                .filter(|action| action.command.is_some())
                .take(8)
                .enumerate()
            {
                self.text(
                    &command_signature(action).expect("filtered command"),
                    panel.x + 24.0 * self.scale,
                    panel.y + (86.0 + index as f32 * 20.0) * self.scale,
                    0.56,
                    TEXT,
                    70,
                );
            }
        }
        let command_y = panel.y + panel.height - 91.0 * self.scale;
        self.text(
            "COMMAND",
            panel.x + 24.0 * self.scale,
            command_y - 19.0 * self.scale,
            0.56,
            MUTED,
            20,
        );
        let input = UiRect {
            x: panel.x + 24.0 * self.scale,
            y: command_y,
            width: panel.width - 48.0 * self.scale,
            height: 35.0 * self.scale,
        };
        self.rounded_panel(input, [0.03, 0.045, 0.045, 0.96], EDGE, 1.0 * self.scale);
        let visible: String = frame
            .admin_input
            .chars()
            .rev()
            .take(72)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        self.text(
            &format!("> {visible}_"),
            input.x + 9.0 * self.scale,
            input.y + 10.0 * self.scale,
            0.66,
            TEXT,
            76,
        );
        for (control, label) in [
            (UiControl::AdminPrev, "PREV"),
            (UiControl::AdminNext, "NEXT"),
            (UiControl::AdminRun, "RUN"),
        ] {
            if !frame.admin_enabled && control != UiControl::AdminRun {
                continue;
            }
            if let Some(rect) = layout.rect(control) {
                self.button(rect, label, frame.hovered == Some(control), false);
            }
        }
        let page = format!(
            "PAGE {} / {}",
            frame.admin_page + 1,
            catalog.items().count().div_ceil(24).max(1)
        );
        if frame.admin_enabled {
            self.text(
                &page,
                panel.x + 208.0 * self.scale,
                panel.y + panel.height - 36.0 * self.scale,
                0.6,
                MUTED,
                25,
            );
        }
    }
}
