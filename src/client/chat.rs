//! Native chat editor and bounded session history. Enter/T opens, Enter submits.
use super::*;
use std::collections::VecDeque;
#[derive(Debug, Default)]
pub(crate) struct Session {
    pub(crate) open: bool,
    pub(crate) input: String,
    lines: VecDeque<String>,
    history: VecDeque<String>,
    history_index: Option<usize>,
    sequence: u64,
    last_message: u64,
}
impl Session {
    pub(crate) fn lines(&self) -> &VecDeque<String> {
        &self.lines
    }
    pub(crate) fn received(&mut self, message: bloxgloom_host_api::chat::Message) {
        if message.id <= self.last_message {
            return;
        }
        self.last_message = message.id;
        self.notice(format!("{}: {}", message.name, message.text));
    }
    pub(crate) fn notice(&mut self, text: String) {
        if self.lines.len() == 64 {
            self.lines.pop_front();
        }
        self.lines.push_back(text);
    }
    fn history(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        self.history_index = if up {
            Some(
                self.history_index
                    .map_or(self.history.len() - 1, |i| i.saturating_sub(1)),
            )
        } else {
            self.history_index
                .and_then(|i| (i + 1 < self.history.len()).then_some(i + 1))
        };
        self.input = self
            .history_index
            .map_or_else(String::new, |i| self.history[i].clone());
    }
}
impl ClientApp {
    /// Called before package/egui/gameplay keyboard handling, for press and release.
    pub(super) fn chat_key(
        &mut self,
        code: KeyCode,
        text: Option<&str>,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        if self.screen != UiScreen::Playing || self.disconnected {
            return false;
        }
        if !self.chat.open {
            if pressed
                && !repeat
                && matches!(code, KeyCode::KeyT | KeyCode::Enter | KeyCode::NumpadEnter)
            {
                self.chat.open = true;
                self.chat.history_index = None;
                self.shift_down = false;
                self.set_grab(false);
                return true;
            }
            return false;
        }
        if !pressed {
            return true;
        }
        match code {
            KeyCode::Escape => {
                self.chat.open = false;
                self.chat.input.clear();
                self.set_grab(true);
            }
            KeyCode::Enter | KeyCode::NumpadEnter if !repeat => {
                let text = self.chat.input.trim().to_owned();
                if bloxgloom_host_api::chat::valid_text(&text)
                    && let Some(sequence) = self.chat.sequence.checked_add(1)
                {
                    self.chat.sequence = sequence;
                    if self.network.send(ClientMessage::Chat {
                        sequence,
                        text: text.clone(),
                    }) {
                        if self.chat.history.back() != Some(&text) {
                            if self.chat.history.len() == 32 {
                                self.chat.history.pop_front();
                            }
                            self.chat.history.push_back(text);
                        }
                        self.chat.input.clear();
                        self.chat.open = false;
                        self.set_grab(true);
                    }
                }
            }
            KeyCode::Backspace => {
                self.chat.input.pop();
            }
            KeyCode::ArrowUp => self.chat.history(true),
            KeyCode::ArrowDown => self.chat.history(false),
            _ => {
                if let Some(text) = text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if self.chat.input.len() + c.len_utf8()
                            <= bloxgloom_host_api::chat::MAX_TEXT_BYTES
                        {
                            self.chat.input.push(c);
                        }
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
