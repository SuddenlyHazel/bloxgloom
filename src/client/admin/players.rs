//! Name completion captures an exact session token before a command is queued.
use crate::{content::Catalog, protocol::PlayerSummary};
use bloxgloom_host_api::actions::CommandArgument;
pub(super) fn token(player: &PlayerSummary) -> String {
    format!("session:{:032x}:{:016x}", player.profile, player.session)
}
pub(super) fn normalize(
    catalog: &Catalog,
    key: &str,
    values: &[&str],
    players: &[PlayerSummary],
) -> Result<Vec<String>, &'static str> {
    let command = catalog
        .command_action(key)
        .and_then(|a| a.command.as_ref())
        .ok_or("Unknown command")?;
    let mut normalized = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        if matches!(command.arguments.get(index), Some(CommandArgument::Player)) {
            let mut matches = players
                .iter()
                .filter(|p| p.name == *value || token(p) == *value);
            let player = matches.next().ok_or("Player is offline or unknown")?;
            if matches.next().is_some() {
                return Err("Ambiguous player name; use an exact session token");
            }
            normalized.push(token(player));
        } else {
            normalized.push((*value).into());
        }
    }
    Ok(normalized)
}
pub(super) fn complete(
    input: &str,
    catalog: &Catalog,
    players: &[PlayerSummary],
) -> Result<String, &'static str> {
    if input.len() > 1024 {
        return Err("Command is too long");
    }
    let parts = super::tokens::spanned(input)?;
    let key = parts.first().ok_or("Enter a player command")?.0.as_str();
    let trailing = input.ends_with(char::is_whitespace);
    if parts.len() == 1 && !trailing {
        return Err("Type a space before completing the player argument");
    }
    let argument = parts.len().saturating_sub(if trailing { 1 } else { 2 });
    let schema = catalog
        .command_action(key)
        .and_then(|a| a.command.as_ref())
        .ok_or("Unknown command")?;
    if !matches!(
        schema.arguments.get(argument),
        Some(CommandArgument::Player)
    ) {
        return Err("This argument does not target a player");
    }
    let prefix = if trailing {
        ""
    } else {
        parts
            .get(argument + 1)
            .map_or("", |(value, _)| value.as_str())
    };
    let mut matches = players
        .iter()
        .filter(|p| p.name.starts_with(prefix) || token(p).starts_with(prefix));
    let player = matches.next().ok_or("No matching online player")?;
    if matches.next().is_some() {
        return Err("Ambiguous player name; type more of the name");
    }
    let cut = if trailing {
        input.len()
    } else {
        parts.last().ok_or("Enter a player argument")?.1
    };
    let value = format!("{}{}", &input[..cut], token(player));
    if value.len() > 1024 {
        return Err("Command is too long");
    }
    Ok(value)
}
