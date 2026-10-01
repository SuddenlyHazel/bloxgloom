use super::*;

#[test]
fn cached_helpers_rebind_to_fresh_invocation_and_restore_budgets() {
    let lua = Lua::new();
    let diagnostics = Diagnostics::install(
        &lua,
        "first:main",
        Execution::new("initialization", 1, "first-call"),
    )
    .unwrap();
    let cached: mlua::Function = lua
        .load("local info, output = log.info, print; return function(n) for i=1,n do info('retained') end; output('cached print') end")
        .set_name("dependency:helper")
        .eval()
        .unwrap();
    cached.call::<()>(MAX_RECORDS + 1).unwrap();
    assert_eq!(diagnostics.buffer.borrow().records.len(), MAX_RECORDS);
    assert_eq!(diagnostics.buffer.borrow().suppressed, 2);
    diagnostics.finish("script_error");
    // An inactive realm cannot leave records in the next call's buffer.
    cached.call::<()>(1).unwrap();
    assert!(diagnostics.buffer.borrow().records.is_empty());
    diagnostics.begin("second:main", Execution::new("observer", 2, "second-call"));
    cached.call::<()>(1).unwrap();
    let buffer = diagnostics.buffer.borrow();
    assert_eq!(buffer.records.len(), 2);
    assert_eq!(buffer.suppressed, 0);
    assert!(
        buffer
            .records
            .iter()
            .all(|record| record.module.contains("dependency:helper"))
    );
    assert_eq!(diagnostics.context.borrow().0, "second:main");
    assert_eq!(diagnostics.context.borrow().1.correlation, "second-call");
    assert_eq!(diagnostics.context.borrow().1.kind, "observer");
}
