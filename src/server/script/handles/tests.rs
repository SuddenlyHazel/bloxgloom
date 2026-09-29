use super::*;

#[test]
fn exact_nominal_handles_compare_intern_and_reject_forgery() {
    let lua = Lua::new();
    for id in [(1 << 53) + 1, u64::MAX] {
        lua.globals().set("id", entity(&lua, id).unwrap()).unwrap();
        lua.globals()
            .set("same", entity(&lua, id).unwrap())
            .unwrap();
        lua.globals()
            .set("revision", revision(&lua, id).unwrap())
            .unwrap();
        lua.load("assert(id == same and id ~= revision); local keys = {[id] = 'yes'}; assert(keys[same] == 'yes'); assert(not pcall(function() id.value = 1 end))")
            .exec().unwrap();
        assert_eq!(entity_value(lua.globals().get("id").unwrap()), Ok(id));
        assert!(entity_value(lua.globals().get("revision").unwrap()).is_err());
    }
    for value in [
        Value::Nil,
        Value::Integer(1),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string("entity:ffffffffffffffff").unwrap()),
    ] {
        assert!(entity_value(value).is_err());
    }
    assert!(entity(&lua, 0).is_err());
}

#[test]
fn tick_helpers_keep_order_and_exact_intervals_above_float_precision() {
    let lua = Lua::new();
    lua.globals()
        .set("earlier", tick(&lua, u64::MAX - 9).unwrap())
        .unwrap();
    lua.globals()
        .set("now", tick(&lua, u64::MAX).unwrap())
        .unwrap();
    lua.globals().set("zero", tick(&lua, 0).unwrap()).unwrap();
    lua.globals()
        .set("revision", revision(&lua, 0).unwrap())
        .unwrap();
    lua.load("assert(earlier:before(now)); assert(not now:before(earlier)); assert(now:elapsed_since(earlier) == 9); assert(not pcall(function() earlier:elapsed_since(now) end)); assert(not pcall(function() now:elapsed_since(zero) end)); assert(not pcall(function() now:before(revision) end)); assert(revision:is_initial())")
        .exec().unwrap();
    assert_eq!(unit_random(0), 0.0);
    assert!(unit_random(u64::MAX) < 1.0);
}

#[test]
fn weak_interning_does_not_retain_unreferenced_handles() {
    let lua = Lua::new();
    let id = entity(&lua, u64::MAX).unwrap();
    let cache: Table = lua.named_registry_value("bloxgloom.handles").unwrap();
    assert!(
        cache
            .raw_get::<Option<AnyUserData>>("entity:ffffffffffffffff")
            .unwrap()
            .is_some()
    );
    drop(id);
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    assert!(
        cache
            .raw_get::<Option<AnyUserData>>("entity:ffffffffffffffff")
            .unwrap()
            .is_none()
    );
}
