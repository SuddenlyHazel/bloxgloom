//! Console convenience; authorization and range checks also run on the server.
use crate::weather::WeatherKind;

pub(super) fn parse(arguments: &[&str]) -> Result<(WeatherKind, u32), &'static str> {
    let arguments = arguments.strip_prefix(&["set"]).unwrap_or(arguments);
    let (kind, seconds) = match arguments {
        [kind] => (*kind, "10"),
        [kind, seconds] => (*kind, *seconds),
        _ => return Err("Usage: weather set <clear|rain|storm> [transition-seconds: 0..60]"),
    };
    let kind = match kind {
        "clear" => WeatherKind::Clear,
        "rain" => WeatherKind::Rain,
        "storm" => WeatherKind::Storm,
        _ => return Err("Weather must be clear, rain, or storm"),
    };
    if seconds.is_empty() || !seconds.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Transition seconds must be a whole number from 0 to 60");
    }
    let seconds: u32 = seconds.parse().map_err(|_| "Invalid transition seconds")?;
    if seconds > 60 {
        return Err("Transition seconds must be a whole number from 0 to 60");
    }
    Ok((kind, seconds * 1_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weather_alias_accepts_bounded_transition_and_rejects_malformed_requests() {
        assert_eq!(parse(&["set", "storm", "0"]), Ok((WeatherKind::Storm, 0)));
        assert_eq!(parse(&["rain"]), Ok((WeatherKind::Rain, 10_000)));
        assert_eq!(parse(&["clear", "60"]), Ok((WeatherKind::Clear, 60_000)));
        for args in [
            vec![],
            vec!["set"],
            vec!["fog"],
            vec!["rain", "61"],
            vec!["storm", "-1"],
            vec!["clear", "NaN"],
            vec!["rain", "1", "extra"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
