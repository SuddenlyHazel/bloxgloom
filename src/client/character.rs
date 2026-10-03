//! Draft UI state, separate from the server-owned appearance replica.
use crate::{
    appearance::{AppearanceState, CharacterRecipe, PackagedAppearance},
    ui::CharacterPanel,
};
use std::time::Instant;

pub(super) struct CharacterEditor {
    draft: Option<CharacterRecipe>,
    model_draft: Option<PackagedAppearance>,
    model_accepted: Option<PackagedAppearance>,
    model_pending: Option<Option<PackagedAppearance>>,
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
            draft: Some(CharacterRecipe::default()),
            model_draft: None,
            model_accepted: None,
            model_pending: None,
            cosmetics: [0; 4],
            accepted: Some(CharacterRecipe::default()),
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
        if self.pending.is_some() || self.model_pending.is_some() {
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
        let model = state.packaged.map(PackagedAppearance::durable);
        if !self.loaded {
            self.model_draft = model;
            self.model_accepted = model;
        } else if self.model_pending == Some(model) {
            self.model_pending = None;
            self.model_accepted = model;
            self.status = "Saved on this server";
        } else if self.model_pending.is_none() && self.model_accepted != model {
            if self.model_draft == self.model_accepted {
                self.model_draft = model;
            }
            self.model_accepted = model;
        }

        let confirmed = Some(state.character.unwrap_or_default());
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
        if self.loaded
            && self.pending.is_none()
            && self.model_pending.is_none()
            && self.model_draft.is_none()
            && recipe.is_none_or(CharacterRecipe::valid)
        {
            self.draft = Some(recipe.unwrap_or_default());
            self.status = "Unapplied changes";
        }
    }
    pub(super) fn edit_model(&mut self, model: Option<u32>) {
        if !self.loaded || self.pending.is_some() || self.model_pending.is_some() {
            return;
        }
        self.model_draft = match model {
            None => None,
            Some(model) => Some(self.model_draft.filter(|p| p.model == model).unwrap_or(
                PackagedAppearance {
                    model,
                    visual: Default::default(),
                },
            )),
        };
        self.status = "Unapplied changes";
    }
    pub(super) fn edit_model_visual(&mut self, visual: bloxgloom_host_api::entity::VisualState) {
        if self.loaded
            && self.pending.is_none()
            && self.model_pending.is_none()
            && let Some(model) = &mut self.model_draft
        {
            model.visual = visual;
            self.status = "Unapplied changes";
        }
    }
    pub(super) fn apply_model(&mut self) -> Option<Option<PackagedAppearance>> {
        if !self.loaded
            || self.pending.is_some()
            || self.model_pending.is_some()
            || self.model_draft == self.model_accepted
        {
            return None;
        }
        self.model_pending = Some(self.model_draft);
        self.status = "Waiting for server confirmation…";
        Some(self.model_draft)
    }
    pub(super) fn clip(&mut self, clip: u8) {
        if clip < 6 && self.clip != clip {
            self.clip = clip;
            self.started = Instant::now();
        }
    }
    pub(super) fn apply(&mut self) -> Option<Option<CharacterRecipe>> {
        if !self.loaded
            || self.pending.is_some()
            || self.model_pending.is_some()
            || self.model_draft.is_some()
            || self.draft == self.accepted
        {
            return None;
        }
        self.pending = Some(self.draft);
        self.status = "Waiting for server confirmation…";
        Some(self.draft)
    }
    pub(super) fn panel(&self) -> CharacterPanel {
        CharacterPanel {
            cosmetics: self.cosmetics,
            packaged: self.model_draft,
            recipe: self.draft,
            can_apply: self.loaded
                && self.pending.is_none()
                && self.model_pending.is_none()
                && (self.model_draft != self.model_accepted
                    || (self.model_draft.is_none() && self.draft != self.accepted)),
            pending: self.pending.is_some() || self.model_pending.is_some(),
            status: self.status,
            clip: self.clip,
            time: self.started.elapsed().as_secs_f32(),
            preview: None,
        }
    }
}

#[cfg(test)]
mod tests;
