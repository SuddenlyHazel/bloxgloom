//! Declared local physical keys resolve through persistent user overrides.
use super::*;
use crate::config::bindings::{self, Bindings, NamedBindings};
use serde::Deserialize;
use std::collections::BTreeSet;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Scope {
    #[default]
    Game,
    Ui,
    Both,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Declaration {
    pub key: String,
    pub event: String,
    #[serde(default = "default_key")]
    pub default: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub open: bool,
}

fn default_key() -> String {
    "B".into()
}

#[derive(Clone, Debug)]
pub(super) struct Binding {
    pub key: String,
    pub event: String,
    pub default: KeyCode,
    pub scope: Scope,
    pub open: bool,
}

pub(super) fn resolve(owner: &str, declarations: Option<Vec<Declaration>>) -> Result<Vec<Binding>> {
    let declarations = declarations.unwrap_or_default();
    if declarations.len() > 16 {
        return Err("too many document bindings");
    }
    let mut keys = BTreeSet::new();
    let mut defaults = BTreeSet::new();
    declarations
        .into_iter()
        .map(|declaration| {
            if !owned(owner, &declaration.key) || !owned(owner, &declaration.event) {
                return Err("binding keys and events must belong to the document package");
            }
            let default = bindings::parse(&declaration.default)
                .ok_or("binding default must be a physical letter")?;
            let mut probe = NamedBindings::default();
            if !probe.bind(&declaration.key, default, Bindings::default()) {
                return Err("binding default conflicts with native controls");
            }
            if !keys.insert(declaration.key.clone()) || !defaults.insert(default) {
                return Err("duplicate document binding key or default");
            }
            Ok(Binding {
                key: declaration.key,
                event: declaration.event,
                default,
                scope: declaration.scope,
                open: declaration.open,
            })
        })
        .collect()
}

impl Binding {
    fn effective_key(&self, named: &NamedBindings, builtins: Bindings) -> Option<KeyCode> {
        let key = named.0.get(&self.key).copied().unwrap_or(self.default);
        let mut probe = NamedBindings::default();
        if !probe.bind(&self.key, key, builtins)
            || named
                .0
                .iter()
                .any(|(other, bound)| other != &self.key && *bound == key)
        {
            return None;
        }
        Some(key)
    }
    fn active(&self, ui_open: bool) -> bool {
        matches!(self.scope, Scope::Both)
            || if ui_open {
                self.scope == Scope::Ui
            } else {
                self.scope == Scope::Game
            }
    }
}

impl Session {
    fn matching_binding(
        &self,
        code: KeyCode,
        named: &NamedBindings,
        builtins: Bindings,
        ui_open: bool,
        keyboard_focus: bool,
    ) -> Option<&Binding> {
        if keyboard_focus
            || ui_open
                && self.focused.is_some_and(|index| {
                    matches!(
                        self.document().nodes[index].kind,
                        Kind::Input | Kind::MultilineInput
                    )
                })
        {
            return None;
        }
        self.document().bindings.iter().find(|binding| {
            binding.active(ui_open) && binding.effective_key(named, builtins) == Some(code)
        })
    }

    /// A recognized key stays local even when its worker is busy; it must not
    /// fall through to a namespaced server command with the same identifier.
    pub(crate) fn binding_reserved(
        &self,
        code: KeyCode,
        named: &NamedBindings,
        builtins: Bindings,
        ui_open: bool,
        keyboard_focus: bool,
    ) -> bool {
        self.matching_binding(code, named, builtins, ui_open, keyboard_focus)
            .is_some()
    }

    pub(crate) fn binding_key(
        &mut self,
        code: KeyCode,
        named: &NamedBindings,
        builtins: Bindings,
        ui_open: bool,
        keyboard_focus: bool,
    ) -> Option<bool> {
        let binding = self
            .matching_binding(code, named, builtins, ui_open, keyboard_focus)?
            .clone();
        self.dispatch_event(binding.event, "pressed".into())
            .then_some(binding.open)
    }

    pub(crate) fn declared_bindings(&self) -> Vec<(String, KeyCode)> {
        self.resources
            .documents
            .iter()
            .flat_map(|document| {
                document
                    .bindings
                    .iter()
                    .map(|binding| (binding.key.clone(), binding.default))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "bindings/tests.rs"]
mod tests;
