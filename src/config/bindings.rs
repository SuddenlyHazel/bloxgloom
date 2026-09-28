//! Local physical-key bindings for existing semantic client actions. These
//! never authorize a server action; the server still validates every request.
use winit::keyboard::KeyCode;

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
        keys.iter().all(|key| {
            !matches!(
                key,
                KeyCode::KeyW | KeyCode::KeyA | KeyCode::KeyS | KeyCode::KeyD
            ) && letter(*key).is_some()
        }) && keys
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
