//! Each live ClientApp owns one session. The joining shell replaces the entire
//! app on retry/switch instead of selectively resetting authoritative state.
//! Never retain package GPU/UI registrations after a failed or closed session.
use super::*;

impl ClientApp {
    pub(super) fn retire_session(&mut self) {
        self.disconnected = true;
        self.network.retire();
        self.player_roster.clear();
        self.roster_revision = 0;
        self.package_ui = None;
        self.visual_session = None;
        // Renderer owns the package UI textures, material and effect pipelines.
        // A later session constructs a new renderer against its frozen catalog.
        self.renderer = None;
        self.pending_commands.clear();
        self.pending_actions.clear();
        self.deferred_actions.clear();
        self.actions = ActionTracker::default();
        self.action_choices.clear();
        self.active_action = None;
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
