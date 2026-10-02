//! Bounded ordered command schemas shared by startup, clients and dispatch.
use super::{CommandPermission, MAX_REQUEST_ARGUMENTS, key};

pub const MAX_COMMAND_ARGUMENTS: usize = 8;
pub const MAX_COMMAND_ALIASES: usize = 4;
pub const MAX_EXACT_INTEGER: i64 = 9_007_199_254_740_991;
mod schema;

/// Finite, canonical floating point value; negative zero is normalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FiniteNumber(u64);
impl FiniteNumber {
    pub fn new(value: f64) -> Option<Self> {
        value
            .is_finite()
            .then(|| Self(if value == 0.0 { 0 } else { value.to_bits() }))
    }
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

pub fn command_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !matches!(
            value,
            "help" | "time" | "weather" | "appearance" | "give" | "spawn"
        )
}
fn command_text(value: &str, max_bytes: u8) -> bool {
    !value.is_empty()
        && value.len() <= usize::from(max_bytes)
        && !value.chars().any(char::is_control)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandArgument {
    /// Exact live profile/session target. Display names are resolved by the client before encoding.
    Player,
    Text {
        max_bytes: u8,
    },
    Integer {
        min: i64,
        max: i64,
    },
    Number {
        min: FiniteNumber,
        max: FiniteNumber,
    },
    ItemKey {
        max_bytes: u8,
    },
    EntityKey {
        max_bytes: u8,
    },
    /// 1..128; a default makes this trailing positional argument optional in
    /// text input only. Wire requests always contain the explicit value.
    Count {
        default: Option<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandValue {
    Player { profile: u128, session: u64 },
    ItemKey(String),
    EntityKey(String),
    Count(u8),
    Text(String),
    Integer(i64),
    Number(FiniteNumber),
}

/// Startup-frozen facet of an empty-target gameplay action. Namespaced action
/// keys are command identities; discovery is never authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub permission: CommandPermission,
    pub arguments: Vec<CommandArgument>,
    pub aliases: Vec<String>,
}

impl Command {
    /// Bound worst-case payload and traversal, not merely a particular request.
    pub fn max_encoded_len(&self) -> Option<usize> {
        if self.arguments.len() > MAX_COMMAND_ARGUMENTS
            || self.aliases.len() > MAX_COMMAND_ALIASES
            || self
                .aliases
                .iter()
                .enumerate()
                .any(|(i, alias)| !command_alias(alias) || self.aliases[..i].contains(alias))
        {
            return None;
        }
        let mut length = 0;
        let mut optional = false;
        for argument in &self.arguments {
            length += match argument {
                CommandArgument::Player if !optional => 24,
                CommandArgument::Text { max_bytes }
                    if !optional && (1..=128).contains(max_bytes) =>
                {
                    1 + usize::from(*max_bytes)
                }
                CommandArgument::Integer { min, max }
                    if !optional
                        && min <= max
                        && *min >= -MAX_EXACT_INTEGER
                        && *max <= MAX_EXACT_INTEGER =>
                {
                    8
                }
                CommandArgument::Number { min, max } if !optional && min.get() <= max.get() => 8,
                CommandArgument::ItemKey { max_bytes }
                | CommandArgument::EntityKey { max_bytes }
                    if !optional && (3..=128).contains(max_bytes) =>
                {
                    1 + usize::from(*max_bytes)
                }
                CommandArgument::Count { default } => {
                    if default.is_some_and(|n| !(1..=128).contains(&n))
                        || (optional && default.is_none())
                    {
                        return None;
                    }
                    optional |= default.is_some();
                    1
                }
                _ => return None,
            };
        }
        (length <= MAX_REQUEST_ARGUMENTS).then_some(length)
    }

    /// Canonical schema-order encoding: text/keys have u8 byte lengths, counts
    /// use one byte, numbers use eight little-endian bytes. No missing fields,
    /// terminators or trailing bytes.
    /// Text defaults are normalized before sending, not inferred by the server.
    pub fn encode_arguments(&self, values: &[&str]) -> Option<Vec<u8>> {
        let max = self.max_encoded_len()?;
        if values.len() > self.arguments.len() {
            return None;
        }
        let mut bytes = Vec::with_capacity(max);
        for (index, argument) in self.arguments.iter().enumerate() {
            match argument {
                CommandArgument::Text { max_bytes } => {
                    let value = *values.get(index)?;
                    if !command_text(value, *max_bytes) {
                        return None;
                    }
                    bytes.push(value.len() as u8);
                    bytes.extend(value.as_bytes());
                }
                CommandArgument::Integer { min, max } => {
                    let token = values.get(index)?;
                    if token.len() > 17 {
                        return None;
                    }
                    let value = token.parse::<i64>().ok()?;
                    if !(*min..=*max).contains(&value) {
                        return None;
                    }
                    bytes.extend(value.to_le_bytes());
                }
                CommandArgument::Number { min, max } => {
                    let token = values.get(index)?;
                    if token.len() > 64 {
                        return None;
                    }
                    let value = FiniteNumber::new(token.parse::<f64>().ok()?)?;
                    if !(min.get()..=max.get()).contains(&value.get()) {
                        return None;
                    }
                    bytes.extend(value.0.to_le_bytes());
                }
                CommandArgument::Player => {
                    let token = values.get(index)?.strip_prefix("session:")?;
                    let (profile, session) = token.split_once(':')?;
                    if profile.len() != 32
                        || session.len() != 16
                        || !profile
                            .bytes()
                            .chain(session.bytes())
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return None;
                    }
                    let profile = u128::from_str_radix(profile, 16).ok()?;
                    let session = u64::from_str_radix(session, 16).ok()?;
                    if profile == 0 || session == 0 {
                        return None;
                    }
                    bytes.extend(profile.to_le_bytes());
                    bytes.extend(session.to_le_bytes());
                }
                CommandArgument::ItemKey { max_bytes }
                | CommandArgument::EntityKey { max_bytes } => {
                    let value = *values.get(index)?;
                    if value.len() > usize::from(*max_bytes) || !key(value) {
                        return None;
                    }
                    bytes.push(value.len() as u8);
                    bytes.extend(value.bytes());
                }
                CommandArgument::Count { default } => {
                    let count = match values.get(index) {
                        Some(value)
                            if value.len() <= 3 && value.bytes().all(|b| b.is_ascii_digit()) =>
                        {
                            value.parse::<u8>().ok()?
                        }
                        Some(_) => return None,
                        None => (*default)?,
                    };
                    if !(1..=128).contains(&count) {
                        return None;
                    }
                    bytes.push(count);
                }
            }
        }
        Some(bytes)
    }

    pub fn decode_arguments(&self, mut bytes: &[u8]) -> Option<Vec<CommandValue>> {
        if bytes.len() > self.max_encoded_len()? {
            return None;
        }
        let mut values = Vec::with_capacity(self.arguments.len());
        for argument in &self.arguments {
            match argument {
                CommandArgument::Integer { min, max } => {
                    let (value, rest) = bytes.split_at_checked(8)?;
                    let value = i64::from_le_bytes(value.try_into().ok()?);
                    if !(*min..=*max).contains(&value) {
                        return None;
                    }
                    values.push(CommandValue::Integer(value));
                    bytes = rest;
                    continue;
                }
                CommandArgument::Number { min, max } => {
                    let (value, rest) = bytes.split_at_checked(8)?;
                    let bits = u64::from_le_bytes(value.try_into().ok()?);
                    let value = FiniteNumber::new(f64::from_bits(bits))?;
                    if value.0 != bits || !(min.get()..=max.get()).contains(&value.get()) {
                        return None;
                    }
                    values.push(CommandValue::Number(value));
                    bytes = rest;
                    continue;
                }
                _ => {}
            }
            if matches!(argument, CommandArgument::Player) {
                let (target, rest) = bytes.split_at_checked(24)?;
                let profile = u128::from_le_bytes(target[..16].try_into().ok()?);
                let session = u64::from_le_bytes(target[16..].try_into().ok()?);
                if profile == 0 || session == 0 {
                    return None;
                }
                values.push(CommandValue::Player { profile, session });
                bytes = rest;
                continue;
            }
            let (&first, rest) = bytes.split_first()?;
            bytes = rest;
            values.push(match argument {
                CommandArgument::Text { max_bytes } => {
                    let (value, rest) = bytes.split_at_checked(usize::from(first))?;
                    let value = std::str::from_utf8(value).ok()?;
                    if !command_text(value, *max_bytes) {
                        return None;
                    }
                    bytes = rest;
                    CommandValue::Text(value.into())
                }
                CommandArgument::ItemKey { max_bytes }
                | CommandArgument::EntityKey { max_bytes } => {
                    if first > *max_bytes {
                        return None;
                    }
                    let (value, rest) = bytes.split_at_checked(usize::from(first))?;
                    let value = std::str::from_utf8(value).ok()?;
                    if !key(value) {
                        return None;
                    }
                    bytes = rest;
                    if matches!(argument, CommandArgument::ItemKey { .. }) {
                        CommandValue::ItemKey(value.into())
                    } else {
                        CommandValue::EntityKey(value.into())
                    }
                }
                CommandArgument::Count { .. } if (1..=128).contains(&first) => {
                    CommandValue::Count(first)
                }
                _ => return None,
            });
        }
        bytes.is_empty().then_some(values)
    }
}
