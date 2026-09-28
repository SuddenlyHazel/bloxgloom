//! A single bounded startup palette owner. No runtime RGB declaration surface.
use super::*;
use bloxgloom_host_api::appearance::{Appearance, MAX_ADDITIONS};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    permitted: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_, (key, revision, model, fields): (Value, Value, Value, Value)| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permitted {
                    return Err("register_player_appearance requires bloxgloom:content/v1");
                }
                if pending.appearance.is_some() {
                    return Err("duplicate player appearance");
                }
                let key = text(key)?;
                if key.split_once(':').is_none_or(|(owner, local)| {
                    owner != namespace || !super::super::package::manifest::identifier(local)
                }) {
                    return Err("appearance key must belong to the startup package");
                }
                let revision = match revision {
                    Value::Integer(v) => v as f64,
                    Value::Number(v) => v,
                    _ => return Err("appearance revision must be a positive u32"),
                };
                if !revision.is_finite()
                    || revision.fract() != 0.0
                    || !(1.0..=u32::MAX as f64).contains(&revision)
                {
                    return Err("invalid appearance revision");
                }
                let Value::Table(fields) = fields else {
                    return Err("appearance palettes must be a table");
                };
                let mut palettes = [None, None, None];
                for pair in fields.pairs::<Value, Value>().take(4) {
                    let (key, value) = pair.map_err(|_| "invalid appearance palette")?;
                    let index = match text(key)?.as_str() {
                        "skins" => 0,
                        "shirts" => 1,
                        "pants" => 2,
                        _ => return Err("unknown appearance palette"),
                    };
                    let colors = dense_array(value, MAX_ADDITIONS)?;
                    let mut palette = Vec::with_capacity(colors.len());
                    for color in colors {
                        let values = dense_array(color, 3)?;
                        if values.len() != 3 {
                            return Err("appearance color requires three components");
                        }
                        let mut rgb = [0.0; 3];
                        for (index, value) in values.into_iter().enumerate() {
                            rgb[index] = match value {
                                Value::Integer(v) => v as f32,
                                Value::Number(v) => v as f32,
                                _ => return Err("appearance color requires numbers"),
                            };
                        }
                        palette.push(rgb);
                    }
                    palettes[index] = Some(palette);
                }
                let appearance = Appearance {
                    key,
                    revision: revision as u32,
                    model: text(model)?,
                    palettes: palettes.map(|palette| palette.unwrap_or_default()),
                };
                appearance
                    .validate()
                    .map_err(|_| "invalid appearance bounds or model")?;
                pending.appearance = Some(appearance);
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}

fn dense_array(value: Value, limit: usize) -> Result<Vec<Value>, &'static str> {
    let Value::Table(table) = value else {
        return Err("expected appearance array");
    };
    let mut values = vec![None; limit];
    let mut count = 0;
    for pair in table.pairs::<Value, Value>().take(limit + 1) {
        let (key, value) = pair.map_err(|_| "invalid appearance array")?;
        let Value::Integer(key) = key else {
            return Err("appearance array keys must be integers");
        };
        if key < 1 || key as usize > limit {
            return Err("appearance array too large");
        }
        values[key as usize - 1] = Some(value);
        count += 1;
    }
    if count > limit || values[..count].iter().any(Option::is_none) {
        return Err("appearance array must be dense");
    }
    Ok(values.into_iter().take(count).map(Option::unwrap).collect())
}
