//! Closed, bounded block rain profiles and insect habitat declarations.
use super::super::values::text;
use bloxgloom_host_api::content::{Acoustics, Habitat, ImpactProfile, RainSurface};
use mlua::{Table, Value};

pub(super) fn decode(value: Value) -> Result<Acoustics, &'static str> {
    let Value::Table(table) = value else {
        return Err("acoustics must be a table");
    };
    fields(&table, &["surface", "habitat", "impact"])?;
    let surface = RainSurface::parse(&text(
        table
            .raw_get("surface")
            .map_err(|_| "invalid acoustic surface")?,
    )?)
    .ok_or("unknown acoustic surface")?;
    let habitat = match table
        .raw_get::<Value>("habitat")
        .map_err(|_| "invalid habitat")?
    {
        Value::Nil => Habitat::None,
        value => match text(value)?.as_str() {
            "none" => Habitat::None,
            "ground" => Habitat::Ground,
            "canopy" => Habitat::Canopy,
            _ => return Err("unknown insect habitat"),
        },
    };
    let impact = match table
        .raw_get::<Value>("impact")
        .map_err(|_| "invalid impact")?
    {
        Value::Nil => None,
        Value::Table(table) => {
            fields(
                &table,
                &[
                    "gain",
                    "click",
                    "frequency_hz",
                    "damping_per_s",
                    "resonance",
                    "lowpass_hz",
                ],
            )?;
            let profile = ImpactProfile {
                gain: number(&table, "gain")?,
                click: number(&table, "click")?,
                frequency_hz: pair(&table, "frequency_hz")?,
                damping_per_s: pair(&table, "damping_per_s")?,
                resonance: number(&table, "resonance")?,
                lowpass_hz: number(&table, "lowpass_hz")?,
            };
            if !profile.valid() {
                return Err("impact profile out of bounds");
            }
            Some(profile)
        }
        _ => return Err("impact must be a table"),
    };
    Ok(Acoustics {
        surface,
        habitat,
        impact,
    })
}
fn fields(table: &Table, allowed: &[&str]) -> Result<(), &'static str> {
    for (index, field) in table.clone().pairs::<Value, Value>().enumerate() {
        if index >= allowed.len() {
            return Err("too many acoustic fields");
        }
        let (key, _) = field.map_err(|_| "invalid acoustic field")?;
        if !allowed.contains(&text(key)?.as_str()) {
            return Err("unknown acoustic field");
        }
    }
    Ok(())
}
fn scalar(value: Value) -> Result<f32, &'static str> {
    let v = match value {
        Value::Number(v) => v,
        Value::Integer(v) => v as f64,
        _ => return Err("expected acoustic number"),
    };
    if !v.is_finite() {
        return Err("nonfinite acoustic number");
    }
    Ok(v as f32)
}
fn number(table: &Table, key: &str) -> Result<f32, &'static str> {
    scalar(table.raw_get(key).map_err(|_| "invalid acoustic number")?)
}
fn pair(table: &Table, key: &str) -> Result<[f32; 2], &'static str> {
    let Value::Table(table) = table.raw_get(key).map_err(|_| "invalid acoustic pair")? else {
        return Err("expected acoustic pair");
    };
    if table.raw_len() != 2 || table.clone().pairs::<Value, Value>().take(3).count() != 2 {
        return Err("acoustic pair requires two entries");
    }
    Ok([
        scalar(table.raw_get(1).map_err(|_| "invalid pair")?)?,
        scalar(table.raw_get(2).map_err(|_| "invalid pair")?)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acoustic_options_are_closed_and_do_not_invoke_metatables() {
        let lua = mlua::Lua::new();
        let valid = lua
            .load("return {surface='water',habitat='ground'}")
            .eval()
            .unwrap();
        assert_eq!(decode(valid).unwrap().surface, RainSurface::Water);
        for source in [
            "return {surface='mystery'}",
            "return {surface='wood',habitat='ocean'}",
            "return {surface='wood',extra=1}",
            "return {surface='wood',impact={gain=0/0,click=1,frequency_hz={100,200},damping_per_s={20,30},resonance=1,lowpass_hz=4000}}",
            "return {surface='wood',impact={gain=1,click=1,frequency_hz={100,200,300},damping_per_s={20,30},resonance=1,lowpass_hz=4000}}",
            "return setmetatable({}, {__index=function() error('metamethod called') end})",
        ] {
            let value = lua.load(source).eval().unwrap();
            assert!(decode(value).is_err());
        }
    }
}
