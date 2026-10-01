//! Console convenience; authorization and range checks also run on the server.
use crate::weather::WeatherKind;

pub(super) fn parse(arguments: &[&str]) -> Result<(WeatherKind, u32), &'static str> {
    let arguments = arguments.strip_prefix(&["set"]).unwrap_or(arguments);
    let (kind, seconds, severity) = match arguments {
        [kind] => (*kind, "10", "normal"),
        [kind, seconds] => (*kind, *seconds, "normal"),
        [kind, seconds, severity] => (*kind, *seconds, *severity),
        _ => {
            return Err(
                "Usage: weather set <clear|rain|storm> [transition-seconds: 0..60] [storm severity: mild|normal|severe]",
            );
        }
    };
    let kind = match kind {
        "clear" => WeatherKind::Clear,
        "rain" => WeatherKind::Rain,
        "storm" => WeatherKind::Storm,
        _ => return Err("Weather must be clear, rain, or storm"),
    };
    let kind = match (kind, severity) {
        (kind, "normal") => kind,
        (WeatherKind::Storm, "mild") => WeatherKind::StormMild,
        (WeatherKind::Storm, "severe") => WeatherKind::StormSevere,
        _ => {
            return Err("Storm severity must be mild, normal, or severe; it applies only to storm");
        }
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
        assert_eq!(
            parse(&["storm", "0", "severe"]),
            Ok((WeatherKind::StormSevere, 0))
        );
        assert_eq!(
            parse(&["storm", "5", "mild"]),
            Ok((WeatherKind::StormMild, 5_000))
        );
        for args in [
            vec![],
            vec!["set"],
            vec!["fog"],
            vec!["rain", "61"],
            vec!["storm", "-1"],
            vec!["clear", "NaN"],
            vec!["rain", "1", "extra"],
            vec!["rain", "1", "severe"],
            vec!["storm", "1", "extreme"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
