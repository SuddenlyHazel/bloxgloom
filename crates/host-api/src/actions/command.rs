//! Bounded ordered command schemas shared by startup, clients and dispatch.
use super::{CommandPermission, MAX_REQUEST_ARGUMENTS, key};

pub const MAX_COMMAND_ARGUMENTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandArgument {
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
    ItemKey(String),
    EntityKey(String),
    Count(u8),
}

/// Startup-frozen facet of an empty-target gameplay action. Namespaced action
/// keys are command identities; discovery is never authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub permission: CommandPermission,
    pub arguments: Vec<CommandArgument>,
}

impl Command {
    /// Bound worst-case payload and traversal, not merely a particular request.
    pub fn max_encoded_len(&self) -> Option<usize> {
        if self.arguments.len() > MAX_COMMAND_ARGUMENTS {
            return None;
        }
        let mut length = 0;
        let mut optional = false;
        for argument in &self.arguments {
            length += match argument {
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

    /// Canonical schema-order encoding: keys are u8 byte length + ASCII bytes;
    /// counts are one byte. No missing wire fields, terminators or trailing bytes.
    /// Text defaults are normalized before sending, not inferred by the server.
    pub fn encode_arguments(&self, values: &[&str]) -> Option<Vec<u8>> {
        let max = self.max_encoded_len()?;
        if values.len() > self.arguments.len() {
            return None;
        }
        let mut bytes = Vec::with_capacity(max);
        for (index, argument) in self.arguments.iter().enumerate() {
            match argument {
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
            let (&first, rest) = bytes.split_first()?;
            bytes = rest;
            values.push(match argument {
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
