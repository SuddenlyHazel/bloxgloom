//! Bounded placement-state choices sharing one host-owned machine footprint.
use super::*;

pub(super) fn parse(
    value: Value,
    active_option: Value,
    block: &bloxgloom_host_api::content::Block,
    default_state: &str,
    cells: &[FootprintCell],
    fueled: bool,
) -> Result<Vec<api::Variant>, &'static str> {
    let choices = match value {
        Value::Nil => {
            let active = match active_option {
                Value::Nil => default_state.to_owned(),
                value => text(value)?,
            };
            vec![(default_state.to_owned(), active)]
        }
        Value::Table(list) => {
            if !active_option.is_nil() {
                return Err("machine variants cannot use top-level active_state");
            }
            let count = ports::sequence_len(&list, 8)?;
            if count < 2 {
                return Err("machine variants need two to eight entries");
            }
            let mut choices = Vec::with_capacity(count);
            for index in 1..=count {
                let entry: mlua::Table =
                    list.raw_get(index).map_err(|_| "invalid machine variant")?;
                if entry.metatable().is_some() {
                    return Err("machine variant cannot have a metatable");
                }
                let state = text(field(&entry, "state")?)?;
                let active = match field(&entry, "active_state")? {
                    Value::Nil => state.clone(),
                    value => text(value)?,
                };
                if choices.iter().any(|(old, _)| old == &state) {
                    return Err("duplicate machine variant state");
                }
                choices.push((state, active));
            }
            if choices[0].0 != default_state {
                return Err("first machine variant must be the block placement state");
            }
            choices
        }
        _ => return Err("machine variants must be a list"),
    };
    choices
        .into_iter()
        .map(|(state, active)| {
            if !super::super::has_state(block, &state) || !super::super::has_state(block, &active) {
                return Err("machine variant state must belong to its block");
            }
            if state != active && !fueled {
                return Err("machine active state requires fuel");
            }
            Ok(api::Variant {
                placement_state: state.clone(),
                idle: cells
                    .iter()
                    .map(|cell| FootprintCell {
                        offset: cell.offset,
                        state: state.clone(),
                    })
                    .collect(),
                active: cells
                    .iter()
                    .map(|cell| FootprintCell {
                        offset: cell.offset,
                        state: active.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}
