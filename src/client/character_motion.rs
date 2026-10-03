//! Local tool feedback follows requests, never deciding whether world edits succeed.
use super::*;
use crate::render::VisualAvatar;
use std::time::Instant;

#[derive(Default)]
pub(super) struct Motion {
    tool: Option<(bool, Instant)>,
}

impl ClientApp {
    pub(super) fn crouching(&self) -> bool {
        self.owned_entity_id
            .is_some_and(|id| self.player_stances.contains_key(&id))
    }

    pub(super) fn request_crouch(&mut self, crouching: bool) {
        let crouching = crouching && self.screen == UiScreen::Playing && self.grabbed;
        if crouching {
            self.cancel_sprint();
        }
        if crouching != self.crouch_requested {
            self.crouch_requested = crouching;
            self.queue_command(ClientMessage::SetCrouching { crouching });
        }
    }
}

impl Motion {
    pub(super) fn swing(&mut self, right: bool, now: Instant) {
        self.tool = Some((right, now));
    }

    pub(super) fn apply(&mut self, avatar: &mut VisualAvatar, now: Instant) {
        avatar.character_tool = self.tool.and_then(|(right, start)| {
            let elapsed = now.saturating_duration_since(start).as_secs_f32();
            (elapsed < crate::render::character_tool_duration(right)).then_some((right, elapsed))
        });
        if avatar.character_tool.is_none() {
            self.tool = None;
        }
    }
}
