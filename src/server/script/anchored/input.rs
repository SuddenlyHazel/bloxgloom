//! Immutable native inputs and deterministic attempt seeds; no live host access.
use super::*;
pub(super) struct Event<'a> {
    pub kind: &'static str,
    pub state: &'a [u8],
    pub anchor: Option<[i32; 3]>,
    pub tick: Option<u64>,
    pub cells: &'a [api::Cell<'a>],
    pub request: Option<&'a [u8]>,
    pub cause: Option<&'static str>,
    pub maximum: Option<u16>,
}
impl<'a> Event<'a> {
    pub fn new(kind: &'static str, state: &'a [u8]) -> Self {
        Self {
            kind,
            state,
            anchor: None,
            tick: None,
            cells: &[],
            request: None,
            cause: None,
            maximum: None,
        }
    }
    pub fn seed(&self) -> u64 {
        let mut seed = Seed::new().bytes(self.kind.as_bytes()).bytes(self.state);
        if let Some(anchor) = self.anchor {
            for coordinate in anchor {
                seed = seed.bytes(&coordinate.to_le_bytes());
            }
        }
        if let Some(tick) = self.tick {
            seed = seed.word(tick)
        }
        for cell in self.cells {
            for coordinate in cell.offset {
                seed = seed.bytes(&coordinate.to_le_bytes());
            }
            seed = seed
                .bytes(cell.state.as_bytes())
                .word(u64::from(cell.solid));
        }
        if let Some(request) = self.request {
            seed = seed.bytes(request)
        }
        if let Some(cause) = self.cause {
            seed = seed.bytes(cause.as_bytes())
        }
        if let Some(maximum) = self.maximum {
            seed = seed.word(u64::from(maximum))
        }
        seed.finish()
    }
    pub fn table(&self, lua: &Lua) -> mlua::Result<mlua::Table> {
        let input = lua.create_table()?;
        input.set("kind", self.kind)?;
        if self.kind != "Initialize" {
            input.set("state", lua.create_string(self.state)?)?;
        }
        if let Some(anchor) = self.anchor {
            input.set("anchor", coordinates(lua, anchor)?)?;
        }
        if let Some(tick) = self.tick {
            input.set("tick", crate::server::script::handles::tick(lua, tick)?)?;
        }
        if self.kind == "React" {
            let cells = lua.create_table()?;
            for (index, cell) in self.cells.iter().enumerate() {
                let value = lua.create_table()?;
                value.set("offset", coordinates(lua, cell.offset)?)?;
                value.set("state", cell.state)?;
                value.set("solid", cell.solid)?;
                value.set_readonly(true);
                cells.raw_set(index + 1, value)?;
            }
            cells.set_readonly(true);
            input.set("cells", cells)?;
        }
        if let Some(request) = self.request {
            input.set("request", lua.create_string(request)?)?;
        }
        if let Some(cause) = self.cause {
            input.set("cause", cause)?;
        }
        if let Some(maximum) = self.maximum {
            input.set("maximum", maximum)?;
        }
        input.set_readonly(true);
        Ok(input)
    }
}
fn coordinates(lua: &Lua, values: [i32; 3]) -> mlua::Result<mlua::Table> {
    let t = lua.create_sequence_from(values)?;
    t.set_readonly(true);
    Ok(t)
}
