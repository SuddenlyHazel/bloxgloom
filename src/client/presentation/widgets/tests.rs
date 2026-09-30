use super::*;

#[test]
fn dynamic_worker_rejects_sparse_metatable_and_unknown_widget_records() {
    let lua = Lua::new();
    for expression in [
        "{[2]={id='a',kind='label',style='demo:style'}}",
        "{[1]={id='a',kind='label',style='demo:style'},extra=true}",
        "setmetatable({{id='a',kind='label',style='demo:style'}},{})",
        "{setmetatable({id='a',kind='label',style='demo:style'},{})}",
        "{{id='a',kind='label',style='demo:style',callback=function()end}}",
        "{{id='a',kind='select',style='demo:style',options={[2]={key='one',label='One'}}}}",
        "{{id='a',kind='select',style='demo:style',options={setmetatable({key='one',label='One'},{})}}}",
        "{{id='a',kind='slider',style='demo:style',value=0/0}}",
    ] {
        let value = lua
            .load(format!("return {expression}"))
            .eval::<Value>()
            .unwrap();
        assert!(nodes(value).is_err(), "accepted {expression}");
    }
    let valid = lua
        .load("return {{id='a',kind='multiline_input',style='demo:style',text='first\\nsecond'}}")
        .eval::<Value>()
        .unwrap();
    assert_eq!(nodes(valid).unwrap().len(), 1);
    assert!(
        nodes(Value::Table(lua.create_table().unwrap()))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn editable_value_decoder_accepts_only_finite_bounded_utf8_values() {
    assert_eq!(value(Value::Boolean(true)).unwrap(), "true");
    assert_eq!(value(Value::Number(0.5)).unwrap(), "0.5");
    assert!(value(Value::Number(f64::INFINITY)).is_err());
    assert!(value(Value::Number(f64::NAN)).is_err());
    let lua = Lua::new();
    assert!(value(Value::String(lua.create_string([255]).unwrap())).is_err());
    assert!(value(Value::String(lua.create_string("é".repeat(513)).unwrap())).is_err());
    assert!(value(Value::String(lua.create_string("first\tsecond").unwrap())).is_err());
    assert_eq!(
        value(Value::String(lua.create_string("first\nsecond").unwrap())).unwrap(),
        "first\nsecond"
    );
    assert_eq!(
        value(Value::Number(f64::MAX))
            .unwrap()
            .parse::<f64>()
            .unwrap(),
        f64::MAX
    );
}
