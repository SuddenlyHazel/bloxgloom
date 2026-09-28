//! Constant-size, raw-table-only player policy declaration. Rejected declarations
//! poison the startup even when Luau catches the callback error with pcall.
use super::*;
use crate::content::player::Selection;
use bloxgloom_host_api::player::{Body, MotionRates, PlayerRules, SpawnSearch};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    permitted: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, fields): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !permitted {
                return Err("register_player_rules requires bloxgloom:content/v1");
            }
            if pending.player_rules.is_some() {
                return Err("duplicate player rules selection");
            }
            let key = text(key)?;
            if key.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::super::package::manifest::identifier(local)
            }) {
                return Err("player rules key must belong to the startup package namespace");
            }
            let revision = number(revision)?;
            if revision.fract() != 0.0 || !(1.0..=u32::MAX as f64).contains(&revision) {
                return Err("player rules revision must be a positive u32");
            }
            let Value::Table(table) = fields else {
                return Err("player rules fields must be a table");
            };
            const FIELDS: [&[u8]; 9] = [
                b"half_width",
                b"foot_inset",
                b"middle_height",
                b"head_height",
                b"intent_blocks_per_second",
                b"budget_blocks_per_second",
                b"headroom",
                b"max_rise",
                b"eye_height",
            ];
            let mut values = [None; 9];
            // One lookahead rejects excess input without traversing an arbitrary table.
            for pair in table.pairs::<Value, Value>().take(FIELDS.len() + 1) {
                let (key, value) = pair.map_err(|_| "invalid player rules field")?;
                let Value::String(key) = key else {
                    return Err("invalid player rules field name");
                };
                let index = FIELDS
                    .iter()
                    .position(|field| *field == key.as_bytes().as_ref())
                    .ok_or("unknown player rules field")?;
                values[index] = Some(number(value)?);
            }
            if values.iter().any(Option::is_none) {
                return Err("all player rules fields are required");
            }
            let values = values.map(Option::unwrap);
            for index in [6, 7] {
                if values[index].fract() != 0.0 || !(1.0..=1024.0).contains(&values[index]) {
                    return Err("invalid player rules spawn integer");
                }
            }
            let rules = PlayerRules::new(
                Body {
                    half_width: values[0] as f32,
                    foot_inset: values[1] as f32,
                    middle_height: values[2] as f32,
                    head_height: values[3] as f32,
                },
                MotionRates {
                    intent_blocks_per_second: values[4] as f32,
                    budget_blocks_per_second: values[5],
                },
                SpawnSearch {
                    headroom: values[6] as i32,
                    max_rise: values[7] as i32,
                },
                values[8] as f32,
            )
            .map_err(|_| "invalid player rules bounds")?;
            let selection = Selection {
                key,
                revision: revision as u32,
                rules,
            };
            selection
                .validate()
                .map_err(|_| "invalid player rules selection")?;
            pending.player_rules = Some(selection);
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

fn number(value: Value) -> Result<f64, &'static str> {
    let number = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err("player rules values must be numbers"),
    };
    if !number.is_finite() {
        return Err("player rules values must be finite");
    }
    Ok(number)
}
