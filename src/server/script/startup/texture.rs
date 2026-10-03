//! Closed texture presentation options; no implicit inference from cutout alpha.
use bloxgloom_host_api::content::FoliageShading;
use mlua::Value;

pub(super) fn options(value: Value) -> Result<(bool, FoliageShading), &'static str> {
    let mut cutout = false;
    let mut foliage = FoliageShading::default();
    let table = match value {
        Value::Nil => return Ok((cutout, foliage)),
        Value::Table(table) => table,
        _ => return Err("texture options must be a table"),
    };
    for (index, pair) in table.pairs::<Value, Value>().enumerate() {
        if index >= 3 {
            return Err("unknown texture option");
        }
        let (key, value) = pair.map_err(|_| "invalid texture option")?;
        let Value::String(key) = key else {
            return Err("invalid texture option key");
        };
        match key.as_bytes().as_ref() {
            b"alpha_cutout" => {
                if let Value::Boolean(value) = value {
                    cutout = value;
                } else {
                    return Err("alpha_cutout must be boolean");
                }
            }
            b"foliage_wrap" => foliage.wrap = unit(value)?,
            b"foliage_transmission" => foliage.transmission = unit(value)?,
            _ => return Err("unknown texture option"),
        }
    }
    Ok((cutout, foliage))
}

fn unit(value: Value) -> Result<f32, &'static str> {
    let value = match value {
        Value::Integer(v) => v as f64,
        Value::Number(v) => v,
        _ => return Err("foliage shading must be a number in 0..1"),
    };
    if !value.is_finite()
        || !(0.0..=1.0).contains(&value)
        || (value == 0.0 && value.is_sign_negative())
    {
        return Err("foliage shading must be a number in 0..1");
    }
    Ok(value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_material_options_are_explicit_and_bounded() {
        let lua = mlua::Lua::new();
        let (cutout, foliage) =
            options(lua.load("return {alpha_cutout=true}").eval().unwrap()).unwrap();
        assert!(cutout);
        assert_eq!(foliage, FoliageShading::default());
        let (_, foliage) = options(
            lua.load("return {foliage_wrap=0.35, foliage_transmission=0.28}")
                .eval()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            foliage,
            FoliageShading {
                wrap: 0.35,
                transmission: 0.28
            }
        );
        for source in [
            "return {foliage_wrap=-1}",
            "return {foliage_transmission=2}",
            "return {foliage_wrap=0/0}",
            "return {foliage_wrap='0.5'}",
            "return {unknown=true}",
        ] {
            assert!(options(lua.load(source).eval().unwrap()).is_err());
        }
    }
}
