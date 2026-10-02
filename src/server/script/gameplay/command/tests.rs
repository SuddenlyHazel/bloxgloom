use super::*;
fn schema(source: &str) -> Result<Option<Command>, &'static str> {
    let lua = Lua::new();
    let value = lua.load(source).eval::<Value>().unwrap();
    declaration(mlua::Variadic::from_iter([value]))
}
#[test]
fn typed_schemas_and_aliases_are_dense_bounded_and_finite() {
    let command = schema(r#"return {permission='Player',aliases={'tune'},arguments={{kind='text',max_bytes=48},{kind='integer',min=-10,max=20},{kind='number',min=0,max=1.5},{kind='count',default=1}}}"#).unwrap().unwrap();
    assert_eq!(command.aliases, ["tune"]);
    assert_eq!(command.arguments.len(), 4);
    for source in [
        "{permission='Player',aliases={'help'}}",
        "{permission='Player',aliases={'a','a'}}",
        "{permission='Player',aliases={[2]='a'}}",
        "{permission='Player',aliases={'a','b','c','d','e'}}",
        "{permission='Player',arguments={{kind='text',max_bytes=0}}}",
        "{permission='Player',arguments={{kind='integer',min=-1.5,max=1}}}",
        "{permission='Player',arguments={{kind='integer',min=0,max=9007199254740992}}}",
        "{permission='Player',arguments={{kind='number',min=0,max=0/0}}}",
        "{permission='Player',arguments={{kind='number',min=10,max=0}}}",
        "{permission='Player',arguments={{kind='number',min=0,max=1,default=1}}}",
        "{permission='Player',arguments={{kind='text',max_bytes=8,min=0}}}",
        "{permission='Player',arguments={{kind='count',default=1},{kind='text',max_bytes=8}}}",
        "setmetatable({permission='Player'}, {})",
    ] {
        assert!(schema(&format!("return {source}")).is_err(), "{source}");
    }
}
