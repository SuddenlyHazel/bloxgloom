use super::*;
#[test]
fn bytecode_keys_include_source_and_eviction_is_bounded() {
    let cache = RefCell::new(Compiled::default());
    let first = Compiled::get(&cache, "same@1:entry", "return 1").unwrap();
    assert!(Rc::ptr_eq(
        &first,
        &Compiled::get(&cache, "same@1:entry", "return 1").unwrap()
    ));
    let changed = Compiled::get(&cache, "same@1:entry", "return 2").unwrap();
    let lua = mlua::Lua::new();
    assert_eq!(lua.load(first.as_slice()).eval::<i64>().unwrap(), 1);
    assert_eq!(lua.load(changed.as_slice()).eval::<i64>().unwrap(), 2);
    for value in 0..MAX_ENTRIES + 1 {
        Compiled::get(&cache, "same@1:entry", &format!("return {}", value + 100)).unwrap();
    }
    assert_eq!(cache.borrow().entries.len(), MAX_ENTRIES);
    assert!(cache.borrow().bytes <= MAX_BYTES);
    assert!(!Rc::ptr_eq(
        &first,
        &Compiled::get(&cache, "same@1:entry", "return 1").unwrap()
    ));
    // Preserve Chunk::eval's expression form as well as returned module chunks.
    assert_eq!(
        lua.load(Compiled::get(&cache, "expr", "2+3").unwrap().as_slice())
            .eval::<i64>()
            .unwrap(),
        5
    );
}
