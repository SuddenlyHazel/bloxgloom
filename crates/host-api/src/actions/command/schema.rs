//! Extended command declaration codec. Legacy schemas keep their original identities.
use super::*;
impl Command {
    pub fn extended(&self) -> bool {
        !self.aliases.is_empty()
            || self.arguments.iter().any(|a| {
                matches!(
                    a,
                    CommandArgument::Text { .. }
                        | CommandArgument::Integer { .. }
                        | CommandArgument::Number { .. }
                )
            })
    }
    /// Bounded, canonical descriptor; payload arguments have a separate codec.
    pub fn schema_bytes(&self) -> Option<Vec<u8>> {
        self.max_encoded_len()?;
        let mut out = vec![self.arguments.len() as u8];
        for argument in &self.arguments {
            match argument {
                CommandArgument::Player => out.extend([3, 0]),
                CommandArgument::ItemKey { max_bytes } => out.extend([0, *max_bytes]),
                CommandArgument::EntityKey { max_bytes } => out.extend([1, *max_bytes]),
                CommandArgument::Count { default } => out.extend([2, default.unwrap_or(0)]),
                CommandArgument::Text { max_bytes } => out.extend([4, *max_bytes]),
                CommandArgument::Integer { min, max } => {
                    out.push(5);
                    out.extend(min.to_le_bytes());
                    out.extend(max.to_le_bytes());
                }
                CommandArgument::Number { min, max } => {
                    out.push(6);
                    out.extend(min.0.to_le_bytes());
                    out.extend(max.0.to_le_bytes());
                }
            }
        }
        if self.extended() {
            out.push(self.aliases.len() as u8);
            for alias in &self.aliases {
                out.push(alias.len() as u8);
                out.extend(alias.bytes());
            }
        }
        Some(out)
    }
    pub fn from_extended_schema(permission: CommandPermission, mut bytes: &[u8]) -> Option<Self> {
        if bytes.len() > 512 {
            return None;
        }
        let (&count, rest) = bytes.split_first()?;
        bytes = rest;
        if usize::from(count) > MAX_COMMAND_ARGUMENTS {
            return None;
        }
        let mut arguments = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let (&kind, rest) = bytes.split_first()?;
            bytes = rest;
            arguments.push(match kind {
                0..=4 => {
                    let (&bound, rest) = bytes.split_first()?;
                    bytes = rest;
                    match kind {
                        0 => CommandArgument::ItemKey { max_bytes: bound },
                        1 => CommandArgument::EntityKey { max_bytes: bound },
                        2 => CommandArgument::Count {
                            default: (bound != 0).then_some(bound),
                        },
                        3 if bound == 0 => CommandArgument::Player,
                        4 => CommandArgument::Text { max_bytes: bound },
                        _ => return None,
                    }
                }
                5 | 6 => {
                    let (bounds, rest) = bytes.split_at_checked(16)?;
                    bytes = rest;
                    if kind == 5 {
                        CommandArgument::Integer {
                            min: i64::from_le_bytes(bounds[..8].try_into().ok()?),
                            max: i64::from_le_bytes(bounds[8..].try_into().ok()?),
                        }
                    } else {
                        let min_bits = u64::from_le_bytes(bounds[..8].try_into().ok()?);
                        let max_bits = u64::from_le_bytes(bounds[8..].try_into().ok()?);
                        let min = FiniteNumber::new(f64::from_bits(min_bits))?;
                        let max = FiniteNumber::new(f64::from_bits(max_bits))?;
                        if min.0 != min_bits || max.0 != max_bits {
                            return None;
                        }
                        CommandArgument::Number { min, max }
                    }
                }
                _ => return None,
            });
        }
        let (&count, rest) = bytes.split_first()?;
        bytes = rest;
        if usize::from(count) > MAX_COMMAND_ALIASES {
            return None;
        }
        let mut aliases = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let (&length, rest) = bytes.split_first()?;
            let (alias, rest) = rest.split_at_checked(usize::from(length))?;
            bytes = rest;
            aliases.push(std::str::from_utf8(alias).ok()?.into());
        }
        let command = Self {
            permission,
            arguments,
            aliases,
        };
        command.max_encoded_len()?;
        (bytes.is_empty() && command.extended()).then_some(command)
    }
}
