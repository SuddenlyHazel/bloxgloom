//! Local physical-key bindings for built-in controls and namespaced commands.
//! These never authorize a server action; the server validates every request.
use std::collections::BTreeMap;
use winit::keyboard::KeyCode;

/// Local, server-independent shortcuts. A key is never sufficient authority to
/// invoke an action: the active session must advertise this namespaced command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NamedBindings(pub BTreeMap<String, KeyCode>);

impl NamedBindings {
    pub fn bind(&mut self, action: &str, key: KeyCode, builtins: Bindings) -> bool {
        if !valid_action_key(action)
            || self.0.len() >= 32 && !self.0.contains_key(action)
            || !allowed_key(key)
            || builtins.action(key).is_some()
            || self
                .0
                .iter()
                .any(|(other, bound)| other != action && *bound == key)
        {
            return false;
        }
        self.0.insert(action.to_owned(), key);
        true
    }

    pub fn action(&self, key: KeyCode) -> Option<&str> {
        self.0
            .iter()
            .find_map(|(action, bound)| (*bound == key).then_some(action.as_str()))
    }

    pub fn sanitized(&self, builtins: Bindings) -> Self {
        let mut result = Self::default();
        for (action, key) in &self.0 {
            result.bind(action, *key, builtins);
        }
        result
    }
}

fn valid_action_key(action: &str) -> bool {
    action.len() <= 128
        && action.split_once(':').is_some_and(|(namespace, name)| {
            !namespace.is_empty()
                && !name.is_empty()
                && namespace.bytes().chain(name.bytes()).all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.' | b'/')
                })
        })
}

fn allowed_key(key: KeyCode) -> bool {
    !matches!(
        key,
        KeyCode::KeyW | KeyCode::KeyA | KeyCode::KeyS | KeyCode::KeyD
    ) && letter(key).is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Inventory,
    KilnInput,
    KilnFuel,
    Drop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bindings {
    pub inventory: KeyCode,
    pub kiln_input: KeyCode,
    pub kiln_fuel: KeyCode,
    pub drop: KeyCode,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            inventory: KeyCode::KeyE,
            kiln_input: KeyCode::KeyR,
            kiln_fuel: KeyCode::KeyF,
            drop: KeyCode::KeyQ,
        }
    }
}

impl Bindings {
    pub fn action(self, key: KeyCode) -> Option<Action> {
        [
            (self.inventory, Action::Inventory),
            (self.kiln_input, Action::KilnInput),
            (self.kiln_fuel, Action::KilnFuel),
            (self.drop, Action::Drop),
        ]
        .into_iter()
        .find_map(|(bound, action)| (bound == key).then_some(action))
    }

    pub fn valid(self) -> bool {
        let keys = [self.inventory, self.kiln_input, self.kiln_fuel, self.drop];
        keys.iter().all(|key| allowed_key(*key))
            && keys
                .iter()
                .enumerate()
                .all(|(i, key)| !keys[..i].contains(key))
    }
}

pub(crate) fn parse(value: &str) -> Option<KeyCode> {
    if value.len() != 1 {
        return None;
    }
    let key = value.as_bytes()[0].to_ascii_uppercase();
    Some(match key {
        b'A' => KeyCode::KeyA,
        b'B' => KeyCode::KeyB,
        b'C' => KeyCode::KeyC,
        b'D' => KeyCode::KeyD,
        b'E' => KeyCode::KeyE,
        b'F' => KeyCode::KeyF,
        b'G' => KeyCode::KeyG,
        b'H' => KeyCode::KeyH,
        b'I' => KeyCode::KeyI,
        b'J' => KeyCode::KeyJ,
        b'K' => KeyCode::KeyK,
        b'L' => KeyCode::KeyL,
        b'M' => KeyCode::KeyM,
        b'N' => KeyCode::KeyN,
        b'O' => KeyCode::KeyO,
        b'P' => KeyCode::KeyP,
        b'Q' => KeyCode::KeyQ,
        b'R' => KeyCode::KeyR,
        b'S' => KeyCode::KeyS,
        b'T' => KeyCode::KeyT,
        b'U' => KeyCode::KeyU,
        b'V' => KeyCode::KeyV,
        b'W' => KeyCode::KeyW,
        b'X' => KeyCode::KeyX,
        b'Y' => KeyCode::KeyY,
        b'Z' => KeyCode::KeyZ,
        _ => return None,
    })
}

pub(crate) fn letter(key: KeyCode) -> Option<char> {
    let name = format!("{key:?}");
    let suffix = name.strip_prefix("Key")?;
    (suffix.len() == 1).then(|| suffix.chars().next()).flatten()
}

#[cfg(test)]
#[path = "bindings/tests.rs"]
mod tests;
