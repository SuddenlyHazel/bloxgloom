//! Bounded diagnostics for one execution attempt, including failed attempts.
use super::Execution;
use mlua::{Lua, Value, Variadic};
use std::{cell::RefCell, rc::Rc};

pub(super) const MAX_RECORDS: usize = 64;
pub(super) const MAX_BYTES: usize = 16 * 1024;
const MAX_MESSAGE: usize = 2048;
#[derive(Clone)]
struct Record {
    level: &'static str,
    module: String,
    message: String,
    fields: String,
}
#[derive(Default)]
struct Buffer {
    records: Vec<Record>,
    bytes: usize,
    suppressed: u64,
    finished: bool,
}
pub(crate) struct Diagnostics {
    buffer: Rc<RefCell<Buffer>>,
    context: Rc<RefCell<(String, Execution)>>,
}
impl Diagnostics {
    pub fn install(lua: &Lua, id: &str, execution: Execution) -> mlua::Result<Self> {
        let buffer = Rc::new(RefCell::new(Buffer::default()));
        let context = Rc::new(RefCell::new((id.to_owned(), execution)));
        let log = lua.create_table()?;
        for level in ["trace", "debug", "info", "warn", "error"] {
            let output = Rc::clone(&buffer);
            let context = Rc::clone(&context);
            log.set(
                level,
                lua.create_function(
                    move |lua, (message, fields): (mlua::LuaString, Option<mlua::Table>)| {
                        // Once full, discard without decoding or copying fields.
                        if full(&output) {
                            return Ok(());
                        }
                        let message = text(message, MAX_MESSAGE)?;
                        let fields = fields
                            .map(encode_fields)
                            .transpose()?
                            .unwrap_or_else(|| "{}".into());
                        let entry = context.borrow().0.clone();
                        push(lua, &output, &entry, level, message, fields);
                        Ok(())
                    },
                )?,
            )?;
        }
        lua.globals().set("log", log)?;
        let output = Rc::clone(&buffer);
        let current = Rc::clone(&context);
        lua.globals().set(
            "print",
            lua.create_function(move |lua, values: Variadic<Value>| {
                if full(&output) {
                    return Ok(());
                }
                let mut message = values
                    .iter()
                    .take(16)
                    .map(|value| primitive(value.clone()))
                    .collect::<Vec<_>>()
                    .join("\t");
                if message.len() > MAX_MESSAGE || values.len() > 16 {
                    let mut end = message.len().min(MAX_MESSAGE - 11);
                    while !message.is_char_boundary(end) {
                        end -= 1;
                    }
                    message.truncate(end);
                    message.push_str("[truncated]");
                }
                let entry = current.borrow().0.clone();
                push(lua, &output, &entry, "info", message, "{}".into());
                Ok(())
            })?,
        )?;
        Ok(Self { buffer, context })
    }
    /// Cached logging functions use the new invocation rather than the
    /// initializer that first installed them. No Lua handles are retained here.
    pub fn begin(&self, id: &str, execution: Execution) {
        self.finish("aborted");
        *self.buffer.borrow_mut() = Buffer::default();
        *self.context.borrow_mut() = (id.to_owned(), execution);
    }

    pub fn finish(&self, outcome: &str) {
        let mut buffer = self.buffer.borrow_mut();
        if buffer.finished {
            return;
        }
        buffer.finished = true;
        let context = self.context.borrow();
        let (entry, execution) = &*context;
        for record in buffer.records.drain(..) {
            let package = entry.split_once(':').map_or(entry.as_str(), |v| v.0);
            macro_rules! emit { ($level:ident) => {
                tracing::$level!(target: "bloxgloom::script", package, module = %record.module,
                    entry = %entry, side = execution.side, callback = execution.kind,
                    invocation = %execution.correlation, outcome, fields = %record.fields,
                    "{}", record.message)
            }; }
            match record.level {
                "trace" => emit!(trace),
                "debug" => emit!(debug),
                "warn" => emit!(warn),
                "error" => emit!(error),
                _ => emit!(info),
            }
        }
        if buffer.suppressed > 0 {
            tracing::warn!(target: "bloxgloom::script", entry = %entry,
                side = execution.side, callback = execution.kind,
                invocation = %execution.correlation, outcome, suppressed = buffer.suppressed,
                "script diagnostics suppressed by invocation budget");
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        self.finish("aborted");
    }
}
fn full(output: &RefCell<Buffer>) -> bool {
    let mut output = output.borrow_mut();
    if output.finished {
        return true;
    }
    if output.records.len() >= MAX_RECORDS || output.bytes >= MAX_BYTES {
        output.suppressed = output.suppressed.saturating_add(1);
        true
    } else {
        false
    }
}
fn push(
    lua: &Lua,
    output: &RefCell<Buffer>,
    entry: &str,
    level: &'static str,
    message: String,
    fields: String,
) {
    let module = (1..=8)
        .find_map(|depth| {
            lua.inspect_stack(depth, |frame| {
                let source = frame.source();
                (source.what != "C")
                    .then(|| {
                        source
                            .source
                            .filter(|source| source.len() <= 512)
                            .map(|source| source.into_owned())
                    })
                    .flatten()
            })
            .flatten()
        })
        .unwrap_or_else(|| entry.into());
    let size = message.len() + fields.len() + module.len();
    let mut buffer = output.borrow_mut();
    if buffer.bytes + size > MAX_BYTES {
        buffer.suppressed = buffer.suppressed.saturating_add(1);
        return;
    }
    buffer.bytes += size;
    buffer.records.push(Record {
        level,
        module,
        message,
        fields,
    });
}
fn invalid(message: &str) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}
fn text(value: mlua::LuaString, max: usize) -> mlua::Result<String> {
    if value.as_bytes().len() > max {
        return Err(invalid("diagnostic string exceeds byte limit"));
    }
    Ok(value.to_str()?.to_owned())
}
fn primitive(value: Value) -> String {
    match value {
        Value::Nil => "nil".into(),
        Value::Boolean(v) => v.to_string(),
        Value::Integer(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => {
            let bytes = v.as_bytes();
            String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_MESSAGE + 1)]).into_owned()
        }
        other => format!("<{}>", other.type_name()),
    }
}
fn encode_fields(fields: mlua::Table) -> mlua::Result<String> {
    if fields.metatable().is_some() {
        return Err(invalid("diagnostic fields must not have a metatable"));
    }
    let mut result = serde_json::Map::new();
    for (index, pair) in fields.pairs::<Value, Value>().enumerate() {
        if index >= 16 {
            return Err(invalid("diagnostic fields exceed 16 entries"));
        }
        let (key, value) = pair?;
        let Value::String(key) = key else {
            return Err(invalid("diagnostic field keys must be strings"));
        };
        let key = text(key, 64)?;
        let value = match value {
            Value::Boolean(v) => v.into(),
            Value::Integer(v) => v.into(),
            Value::Number(v) => serde_json::Number::from_f64(v)
                .ok_or_else(|| invalid("diagnostic numbers must be finite"))?
                .into(),
            Value::String(v) => text(v, 1024)?.into(),
            _ => {
                return Err(invalid(
                    "diagnostic fields must be strings, finite numbers or booleans",
                ));
            }
        };
        result.insert(key, value);
    }
    Ok(serde_json::Value::Object(result).to_string())
}

#[cfg(test)]
mod tests;
