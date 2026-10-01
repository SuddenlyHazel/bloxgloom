//! Draft UI state, separate from the server-owned appearance replica.
use crate::{
    appearance::{AppearanceState, CharacterRecipe},
    ui::CharacterPanel,
};
use std::time::Instant;

pub(super) struct CharacterEditor {
    draft: Option<CharacterRecipe>,
    cosmetics: [u8; 4],
    accepted: Option<CharacterRecipe>,
    loaded: bool,
    pending: Option<Option<CharacterRecipe>>,
    status: &'static str,
    clip: u8,
    started: Instant,
}
impl Default for CharacterEditor {
    fn default() -> Self {
        Self {
            draft: None,
            cosmetics: [0; 4],
            accepted: None,
            loaded: false,
            pending: None,
            status: "Waiting for your player snapshot…",
            clip: 0,
            started: Instant::now(),
        }
    }
}
impl CharacterEditor {
    pub(super) fn open(&mut self, confirmed: Option<AppearanceState>) {
        if self.pending.is_some() {
            return;
        }
        *self = Self::default();
        self.observe(confirmed);
    }
    pub(super) fn observe(&mut self, confirmed: Option<AppearanceState>) {
        let Some(state) = confirmed else {
            return;
        };
        self.cosmetics = state.legacy();
        let confirmed = state.character;
        if !self.loaded {
            self.loaded = true;
            self.draft = confirmed;
            self.accepted = confirmed;
            self.status = "Changes are local until you Apply";
        } else if self.pending == Some(confirmed) {
            self.pending = None;
            self.accepted = confirmed;
            self.status = "Saved on this server";
        } else if self.pending.is_none() && self.accepted != confirmed {
            let clean = self.draft == self.accepted;
            self.accepted = confirmed;
            if clean {
                self.draft = confirmed;
                self.status = "Updated from server";
            } else if self.draft == confirmed {
                self.status = "Matches server appearance";
            }
        }
    }
    pub(super) fn edit(&mut self, recipe: Option<CharacterRecipe>) {
        if self.loaded && self.pending.is_none() && recipe.is_none_or(CharacterRecipe::valid) {
            self.draft = recipe;
            self.status = "Unapplied changes";
        }
    }
    pub(super) fn clip(&mut self, clip: u8) {
        if clip < 5 && self.clip != clip {
            self.clip = clip;
            self.started = Instant::now();
        }
    }
    pub(super) fn apply(&mut self) -> Option<Option<CharacterRecipe>> {
        if !self.loaded || self.pending.is_some() || self.draft == self.accepted {
            return None;
        }
        self.pending = Some(self.draft);
        self.status = "Waiting for server confirmation…";
        Some(self.draft)
    }
    pub(super) fn panel(&self) -> CharacterPanel {
        CharacterPanel {
            cosmetics: self.cosmetics,
            recipe: self.draft,
            can_apply: self.loaded && self.pending.is_none() && self.draft != self.accepted,
            pending: self.pending.is_some(),
            status: self.status,
            clip: self.clip,
            time: self.started.elapsed().as_secs_f32(),
            preview: None,
        }
    }
}

#[cfg(test)]
mod tests;
