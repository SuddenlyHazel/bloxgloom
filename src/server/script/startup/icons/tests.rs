use super::*;
#[test]
fn item_icons_require_owned_content_and_caught_errors_poison_startup() {
    let lua = Lua::new();
    let art: Value = lua
        .load("return {rows={'x'},palette={x={1,0,0,1}}}")
        .eval()
        .unwrap();
    let pending = Rc::new(RefCell::new(Pending::default()));
    let register = declarer(&lua, Rc::clone(&pending), "demo", true).unwrap();
    register.call::<()>(("demo:cell", art.clone())).unwrap();
    assert_eq!(pending.borrow().icons.len(), 1);
    assert!(register.call::<()>(("demo:cell", art.clone())).is_err());
    assert!(pending.borrow().error.is_some());
    assert!(register.call::<()>(("demo:other", art.clone())).is_err());
    for (item, permitted) in [("other:cell", true), ("demo:cell", false)] {
        let pending = Rc::new(RefCell::new(Pending::default()));
        let register = declarer(&lua, Rc::clone(&pending), "demo", permitted).unwrap();
        assert!(register.call::<()>((item, art.clone())).is_err());
        assert!(pending.borrow().icons.is_empty());
    }
}
