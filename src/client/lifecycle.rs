//! Each live ClientApp owns one session. The joining shell replaces the entire
//! app on retry/switch instead of selectively resetting authoritative state.
//! Never retain package GPU/UI registrations after a failed or closed session.
use super::*;

impl ClientApp {
    pub(super) fn retire_session(&mut self) {
        self.audio.retire_session(&self.config);
        self.disconnected = true;
        self.character_editor = Default::default();
        self.character_motion = Default::default();
        self.next_break = None;
        self.player_stances.clear();
        self.crouch_requested = false;
        if let Some(lane) = self.player_services.take() {
            lane.close(self.failure.as_deref().unwrap_or("session retired"));
        }
        self.player_parameter_updates.clear();
        self.network.retire();
        self.player_roster.clear();
        self.roster_revision = 0;
        self.player_states.clear();
        self.player_state_snapshot = 0;
        self.package_ui = None;
        self.observations = Arc::new(Default::default());
        self.visual_session = None;
        self.actor_animator = Default::default();
        // Renderer owns the package UI textures, material and effect pipelines.
        // A later session constructs a new renderer against its frozen catalog.
        self.renderer = None;
        self.pending_commands.clear();
        self.pending_actions.clear();
        self.deferred_actions.clear();
        self.actions = ActionTracker::default();
        self.movement_reset = 0;
        self.action_choices.clear();
        self.active_action = None;
    }

    pub(super) fn poll_player_services(&mut self) {
        let Some(lane) = &self.player_services else {
            return;
        };
        let outputs = lane.replies.try_iter().take(64).collect::<Vec<_>>();
        for mut output in outputs {
            if let Some(ui) = &mut self.package_ui
                && let Err(error) = ui.apply_player_update(&output)
            {
                self.fail_session(error);
                return;
            }
            for update in output.parameters.take_updates() {
                self.player_parameter_updates
                    .insert((update.resource.clone(), update.name.clone()), update);
            }
        }
    }

    pub(super) fn fail_session(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        tracing::error!(%reason, "client session failed");
        // Preserve the first failure rather than replacing it with queue closure.
        if self.failure.is_none() {
            self.failure = Some(reason);
        }
        self.retire_session();
    }
}

#[cfg(test)]
pub(crate) mod tests;
