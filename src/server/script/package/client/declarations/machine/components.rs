//! V39 canonical machine component predicates and outputs.
use super::*;
use bloxgloom_host_api::machine::ComponentValue;

pub(super) fn encode_input(writer: &mut Writer, value: &ComponentMatch) -> Result<(), ScriptError> {
    match value {
        ComponentMatch::Empty => writer.field(&[0]),
        ComponentMatch::Present => writer.field(&[1]),
        ComponentMatch::Exact(value) => {
            writer.field(&[2])?;
            encode_exact(writer, value)
        }
    }
}

pub(super) fn encode_output(
    writer: &mut Writer,
    value: &ComponentOutput,
) -> Result<(), ScriptError> {
    match value {
        ComponentOutput::Empty => writer.field(&[0]),
        ComponentOutput::PreserveInput => writer.field(&[1]),
        ComponentOutput::Exact(value) => {
            writer.field(&[2])?;
            encode_exact(writer, value)
        }
    }
}

pub(super) fn decode_input(reader: &mut Reader<'_>) -> Result<ComponentMatch, ScriptError> {
    match reader.field(1)? {
        [0] => Ok(ComponentMatch::Empty),
        [1] => Ok(ComponentMatch::Present),
        [2] => Ok(ComponentMatch::Exact(decode_exact(reader)?)),
        _ => Err(invalid()),
    }
}

pub(super) fn decode_output(reader: &mut Reader<'_>) -> Result<ComponentOutput, ScriptError> {
    match reader.field(1)? {
        [0] => Ok(ComponentOutput::Empty),
        [1] => Ok(ComponentOutput::PreserveInput),
        [2] => Ok(ComponentOutput::Exact(decode_exact(reader)?)),
        _ => Err(invalid()),
    }
}

fn encode_exact(writer: &mut Writer, value: &ComponentValue) -> Result<(), ScriptError> {
    if !value.valid() {
        return Err(invalid());
    }
    writer.field(&value.version.to_le_bytes())?;
    writer.field(&value.bytes)
}

fn decode_exact(reader: &mut Reader<'_>) -> Result<ComponentValue, ScriptError> {
    let value = ComponentValue {
        version: u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?),
        bytes: reader.field(1024)?.to_vec(),
    };
    value.valid().then_some(value).ok_or_else(invalid)
}
