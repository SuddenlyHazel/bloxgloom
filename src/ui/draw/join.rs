use super::*;

impl UiBuilder<'_> {
    pub(super) fn draw_join(&mut self, frame: &UiFrame<'_>) {
        self.rect(0.0, 0.0, self.width, self.height, [0.035, 0.05, 0.065, 1.0]);
        let x = 28.0 * self.scale;
        let failed = frame.screen == UiScreen::JoinFailed;
        self.text("BLOXGLOOM", x, 28.0 * self.scale, 1.25, GOLD, 32);
        self.text(
            if failed {
                "Unable to join"
            } else {
                "Preparing your session"
            },
            x,
            62.0 * self.scale,
            0.85,
            [0.9, 0.94, 0.96, 1.0],
            32,
        );
        let columns = ((self.width - 2.0 * x) / (18.0 * 0.65 * self.scale)).max(1.0) as usize;
        let rows = ((self.height - 190.0 * self.scale) / (14.0 * self.scale)).max(1.0) as usize;
        // Bound both text traversal and geometry. Preserve explicit newlines;
        // wrap long package-attributed errors instead of running off the window.
        let mut chars = frame.status.unwrap_or("").chars().take(2048).peekable();
        for row in 0..rows.min(32) {
            if chars.peek().is_none() {
                break;
            }
            let mut line = String::new();
            for _ in 0..columns.min(128) {
                let Some(c) = chars.next() else {
                    break;
                };
                if c == '\n' {
                    break;
                }
                line.push(if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                });
            }
            self.text(
                &line,
                x,
                (98.0 + row as f32 * 14.0) * self.scale,
                0.65,
                [0.78, 0.84, 0.87, 1.0],
                128,
            );
        }
        let rect = crate::ui::join_action_rect(
            self.width as u32,
            self.height as u32,
            frame.settings.scale,
        );
        self.button(
            rect,
            if failed {
                "Retry [Enter]"
            } else {
                "Cancel [Esc]"
            },
            true,
            false,
        );
        self.text(
            if failed {
                "Type server. Ctrl+A clears. Close window to exit."
            } else {
                "Verifying content before entering the world."
            },
            x,
            self.height - 24.0 * self.scale,
            0.55,
            [0.65, 0.73, 0.78, 1.0],
            64,
        );
    }
}
