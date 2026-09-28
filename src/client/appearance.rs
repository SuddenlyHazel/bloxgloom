//! Optional non-UI control: BLOXGLOOM_APPEARANCE=skin,shirt,pants selects
//! registered indices on join. Unset leaves the server's saved profile alone.
//! This local preference never supplies RGB, model bytes, or another profile.
use super::Network;
use crate::protocol::ClientMessage;
use std::io;

pub(super) fn apply_environment(network: &Network) -> io::Result<()> {
    let Some(value) = std::env::var_os("BLOXGLOOM_APPEARANCE") else {
        return Ok(());
    };
    if value.len() > 11 {
        return Err(invalid());
    }
    let palettes = parse(value.to_str().ok_or_else(invalid)?)?;
    if !network
        .catalog
        .valid_appearance([palettes[0], palettes[1], palettes[2], 0])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "BLOXGLOOM_APPEARANCE contains an unregistered palette",
        ));
    }
    if !network.send(ClientMessage::SelectAppearance { palettes }) {
        return Err(io::Error::other("appearance request queue unavailable"));
    }
    Ok(())
}

fn parse(value: &str) -> io::Result<[u8; 3]> {
    if value.len() > 11 {
        return Err(invalid());
    }
    let mut parts = value.split(',');
    let mut result = [0; 3];
    for part in &mut result {
        let value = parts.next().ok_or_else(invalid)?;
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        *part = value.parse().map_err(|_| invalid())?;
    }
    if parts.next().is_some() {
        return Err(invalid());
    }
    Ok(result)
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "BLOXGLOOM_APPEARANCE must be three byte indices: skin,shirt,pants",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_is_three_indices_not_raw_colors_or_a_profile() {
        assert_eq!(parse("6,8,6").unwrap(), [6, 8, 6]);
        for bad in [
            "1,2", "1,2,3,4", "1.0,2,3", "-1,2,3", "256,0,0", "#ff00aa", "1, 2,3",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
