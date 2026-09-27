//! Startup-frozen interaction discovery and bounded host UI composition.
//! Discovery is a hint, never authorization. The host checks the request against
//! the actor's current inventory or the target's identity/revision and terrain.
use crate::RegistrationError;
#[cfg(test)]
mod tests;

pub const MAX_ACTIONS: usize = 256;
pub const MAX_TARGET_ACTIONS: usize = 8;
pub const MAX_WIDGETS: usize = 8;
// Tags 2/3/4 are the inventory/mobile/anchored identity envelopes.
pub const REQUEST_TAG: u8 = 5;

/// Shared bounded discovery contract. Hosts additionally resolve all content
/// references before freezing. Canonical action-key order is precedence, not
/// package load order. A future runtime adapter can expose these descriptors.
#[derive(Clone, Debug, Default)]
pub struct Registry(std::collections::BTreeMap<String, std::sync::Arc<Action>>);
impl Registry {
    pub fn register(&mut self, action: Action) -> Result<(), RegistrationError> {
        action.validate()?;
        if self.0.len() >= MAX_ACTIONS
            || self.0.contains_key(&action.key)
            || self.discover(&action.target).count() >= MAX_TARGET_ACTIONS
        {
            return Err(RegistrationError(
                "duplicate action or action capacity exceeded".into(),
            ));
        }
        self.0
            .insert(action.key.clone(), std::sync::Arc::new(action));
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<&std::sync::Arc<Action>> {
        self.0.get(key)
    }
    pub fn values(&self) -> impl Iterator<Item = &std::sync::Arc<Action>> {
        self.0.values()
    }
    /// Resolve forward composition references after collecting all declarations.
    /// Controls may invoke another action for the same targeting context only.
    pub fn validate_composition(&self) -> Result<(), RegistrationError> {
        for action in self.values() {
            if let Some(panel) = &action.panel {
                for widget in &panel.widgets {
                    if let Widget::Button {
                        action: Some(key), ..
                    } = widget
                        && self
                            .get(key)
                            .is_none_or(|other| other.target != action.target)
                    {
                        return Err(RegistrationError(
                            "missing or incompatible composed action".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn discover(
        &self,
        target: &Target,
    ) -> impl Iterator<Item = &std::sync::Arc<Action>> + use<'_> {
        let valid = match target {
            Target::Empty => true,
            Target::Item(k) | Target::Block(k) | Target::Entity(k) => key(k),
        };
        // Reject an oversized lookup before cloning its caller-owned string.
        let target = if valid { target.clone() } else { Target::Empty };
        self.0.values().filter(move |a| valid && a.target == target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Item(String),
    /// An action available without a world target; its recipe still needs the
    /// declared input in the selected inventory slot.
    Empty,
    Block(String),
    Entity(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Explicit consumption/creation, not an inventory move. Only plain stacks
    /// match: component-bearing stacks are never silently stripped or consumed.
    Recipe {
        input: String,
        consume: u16,
        output: String,
        produce: u16,
    },
    /// Fixed, versioned request to the target's registered own-state policy.
    EntityRequest(Vec<u8>),
    /// Open the target's registered inventory descriptor on the client. Slot
    /// controls dispatch the host's exact, revisioned inventory transfer request.
    Inventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Widget {
    Label(String),
    /// Emits a registered action; no arbitrary client-authored effects.
    Button {
        label: String,
        tooltip: String,
        /// None invokes the panel owner. A named action must have the same target.
        action: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    pub title: String,
    pub widgets: Vec<Widget>,
}
impl Panel {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if !text(&self.title, 32) || self.widgets.is_empty() || self.widgets.len() > MAX_WIDGETS {
            return Err(RegistrationError("invalid action panel bounds".into()));
        }
        for widget in &self.widgets {
            let valid = match widget {
                Widget::Label(label) => text(label, 48),
                Widget::Button {
                    label,
                    tooltip,
                    action,
                } => text(label, 32) && text(tooltip, 64) && action.as_ref().is_none_or(|s| key(s)),
            };
            if !valid {
                return Err(RegistrationError("invalid action widget text".into()));
            }
        }
        Ok(())
    }
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
}
fn key(s: &str) -> bool {
    s.len() <= 128
        && s.split_once(':')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:/.-".contains(&b))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub version: u16,
    pub label: String,
    pub target: Target,
    pub operation: Operation,
    pub panel: Option<Panel>,
}
impl Action {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let valid = key(&self.key)
            && self.version != 0
            && text(&self.label, 32)
            && match &self.target {
                Target::Empty => true,
                Target::Item(k) | Target::Block(k) | Target::Entity(k) => key(k),
            };
        let operation = match (&self.target, &self.operation) {
            (
                Target::Item(_) | Target::Empty,
                Operation::Recipe {
                    input,
                    consume,
                    output,
                    produce,
                },
            ) => {
                key(input)
                    && key(output)
                    && (1..=128).contains(consume)
                    && (1..=128).contains(produce)
                    && !matches!(&self.target, Target::Item(k) if k != input)
            }
            (Target::Entity(_) | Target::Block(_), Operation::EntityRequest(bytes)) => {
                !bytes.is_empty() && bytes.len() <= 239
            }
            (Target::Block(_), Operation::Inventory) => self.panel.is_none(),
            _ => false,
        };
        if !valid || !operation {
            return Err(RegistrationError("unsupported action contract".into()));
        }
        if let Some(panel) = &self.panel {
            panel.validate()?;
        }
        Ok(())
    }
    /// Canonical compatibility bytes; includes UI and executable host operations.
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        fn string(out: &mut Vec<u8>, s: &str) {
            out.extend((s.len() as u32).to_le_bytes());
            out.extend(s.as_bytes());
        }
        string(&mut out, &self.key);
        out.extend(self.version.to_le_bytes());
        string(&mut out, &self.label);
        match &self.target {
            Target::Empty => out.push(0),
            Target::Item(k) => {
                out.push(1);
                string(&mut out, k);
            }
            Target::Block(k) => {
                out.push(2);
                string(&mut out, k);
            }
            Target::Entity(k) => {
                out.push(3);
                string(&mut out, k);
            }
        }
        match &self.operation {
            Operation::Recipe {
                input,
                consume,
                output,
                produce,
            } => {
                out.push(0);
                string(&mut out, input);
                out.extend(consume.to_le_bytes());
                string(&mut out, output);
                out.extend(produce.to_le_bytes());
            }
            Operation::EntityRequest(bytes) => {
                out.push(1);
                out.extend((bytes.len() as u32).to_le_bytes());
                out.extend(bytes);
            }
            Operation::Inventory => out.push(2),
        }
        out.push(u8::from(self.panel.is_some()));
        if let Some(panel) = &self.panel {
            string(&mut out, &panel.title);
            out.push(panel.widgets.len() as u8);
            for widget in &panel.widgets {
                match widget {
                    Widget::Label(label) => {
                        out.push(0);
                        string(&mut out, label);
                    }
                    Widget::Button {
                        label,
                        tooltip,
                        action,
                    } => {
                        out.push(1);
                        string(&mut out, label);
                        string(&mut out, tooltip);
                        out.push(u8::from(action.is_some()));
                        if let Some(action) = action {
                            string(&mut out, action);
                        }
                    }
                }
            }
        }
        out
    }
}

/// Bounded request envelope carried by the host's existing durable interaction
/// message/receipt. Target coordinates remain in that message. Entity requests
/// and inventory controls carry exact entity identity and revision; recipes use
/// the actor inventory revision and selected slot. Trailing bytes are forbidden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub key: String,
    pub version: u16,
    pub slot: u8,
    pub inventory_revision: u64,
    pub entity: u64,
    pub entity_revision: u64,
    /// Only Inventory uses these: direction, container slot, count (LE u16).
    pub arguments: Vec<u8>,
}
impl Request {
    pub fn encode(&self) -> Option<Vec<u8>> {
        if !key(&self.key) || self.version == 0 || self.arguments.len() > 4 {
            return None;
        }
        let mut out = vec![REQUEST_TAG, self.key.len() as u8];
        out.extend(self.key.as_bytes());
        out.extend(self.version.to_le_bytes());
        out.push(self.slot);
        out.extend(self.inventory_revision.to_le_bytes());
        out.extend(self.entity.to_le_bytes());
        out.extend(self.entity_revision.to_le_bytes());
        out.push(self.arguments.len() as u8);
        out.extend(&self.arguments);
        Some(out)
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.first() != Some(&REQUEST_TAG) {
            return None;
        }
        let n = usize::from(*bytes.get(1)?);
        if n > 128 || bytes.len() < n + 30 || bytes.len() > n + 34 {
            return None;
        }
        let key = std::str::from_utf8(&bytes[2..n + 2]).ok()?.to_owned();
        let b = &bytes[n + 2..];
        let request = Self {
            key,
            version: u16::from_le_bytes(b[..2].try_into().ok()?),
            slot: b[2],
            inventory_revision: u64::from_le_bytes(b[3..11].try_into().ok()?),
            entity: u64::from_le_bytes(b[11..19].try_into().ok()?),
            entity_revision: u64::from_le_bytes(b[19..27].try_into().ok()?),
            arguments: b[28..].to_vec(),
        };
        if usize::from(b[27]) != request.arguments.len() || request.encode()?.as_slice() != bytes {
            return None;
        }
        Some(request)
    }
}
