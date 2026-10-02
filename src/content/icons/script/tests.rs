use super::*;
#[test]
fn icon_decoder_rejects_sparse_unknown_oversized_and_nonfinite_art() {
    let lua = mlua::Lua::new();
    let valid = decode(
        "demo:cell".into(),
        lua.load("return {rows={'x'},palette={x={1,0,0,1}}}")
            .eval()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(valid.rows, ["x"]);
    for source in [
        "return {rows={[1]='x',[3]='x'},palette={x={1,0,0,1}}}",
        "return {rows={'z'},palette={x={1,0,0,1}}}",
        "return {rows={string.rep('x',33)},palette={x={1,0,0,1}}}",
        "return {rows={'x'},palette={x={1,0,0,0/0}}}",
        "return {rows={'x'},palette={x={1,0,0,1}},extra=true}",
        "return {rows={'x'},palette={x={[1]=1,[2]=0,[3]=0,[4]=1,extra=true}}}",
    ] {
        assert!(
            decode("demo:cell".into(), lua.load(source).eval().unwrap()).is_err(),
            "accepted {source}"
        );
    }
}
