//! Bounded immutable bytecode cache. Keys include exact source, not just a name.
use mlua::chunk::Compiler;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
#[derive(Default)]
pub(in crate::server::script) struct Compiled {
    entries: VecDeque<(String, String, Rc<Vec<u8>>)>,
    bytes: usize,
}
impl Compiled {
    pub fn get(cache: &RefCell<Self>, id: &str, source: &str) -> mlua::Result<Rc<Vec<u8>>> {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache
            .entries
            .iter()
            .position(|(name, text, _)| name == id && text == source)
        {
            let entry = cache.entries.remove(index).expect("located entry");
            let bytes = Rc::clone(&entry.2);
            cache.entries.push_back(entry);
            return Ok(bytes);
        }
        let bytes = Rc::new(
            Compiler::new()
                .compile(format!("return {source}"))
                .or_else(|_| Compiler::new().compile(source))?,
        );
        let size = id.len() + source.len() + bytes.len();
        if size <= MAX_BYTES {
            while cache.bytes + size > MAX_BYTES || cache.entries.len() >= MAX_ENTRIES {
                let (name, text, code) = cache.entries.pop_front().expect("nonempty cache");
                cache.bytes -= name.len() + text.len() + code.len();
            }
            cache.bytes += size;
            cache
                .entries
                .push_back((id.into(), source.into(), Rc::clone(&bytes)));
        }
        Ok(bytes)
    }
}
