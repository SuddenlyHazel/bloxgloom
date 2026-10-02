use super::*;

#[test]
fn malformed_custom_processing_is_rejected_at_the_luau_boundary() {
    let lua = Lua::new();
    let slots = [
        Some(api::Slot {
            item: "demo:raw",
            count: 2,
            has_components: false,
            components: None,
            stack_key: 0,
        }),
        None,
    ];
    let input = "{slot=1,count=1}";
    let output = "{slot=2,item='demo:made',count=1}";
    for proposal in [
        format!("inputs={{}},outputs={{{output}}}"),
        format!("inputs={{{input},{input}}},outputs={{}}"),
        format!("inputs={{{input}}},outputs={{{output},{output}}}"),
        "inputs={{slot=2,count=1}},outputs={}".into(),
        "inputs={{slot=1,count=3}},outputs={}".into(),
        format!("inputs={{{input}}},outputs={{{{slot=3,item='demo:made',count=1}}}}"),
        format!("inputs={{{input}}},outputs={{{{slot=2,item='demo:made',count=129}}}}"),
        format!(
            "inputs={{{input}}},outputs={{{{slot=2,item='demo:made',count=1,components={{version=0,bytes='x'}}}}}}"
        ),
        format!(
            "inputs={{{input}}},outputs={{{{slot=2,item='demo:made',count=1,components={{version=1,bytes=''}}}}}}"
        ),
        format!("inputs={{{input}, extra=true}},outputs={{}}"),
    ] {
        let work = lua
            .load(format!("return {{{proposal}}}"))
            .eval::<mlua::Table>()
            .unwrap();
        assert!(
            transaction::parse(&work, &slots).is_err(),
            "accepted {proposal}"
        );
    }
}
