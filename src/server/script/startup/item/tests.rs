use super::*;

#[test]
fn hexadecimal_schema_fingerprint_is_exact_and_cannot_mix_encodings() {
    let lua = mlua::Lua::new();
    let decode = |fields: &str| {
        let value = lua
            .load(format!("return {{version=1,max_bytes=4,{fields}}}"))
            .eval::<Value>()
            .unwrap();
        component_schema(value)
    };
    let expected = Components::Opaque {
        version: 1,
        fingerprint: u64::MAX,
        max_bytes: 4,
        required: false,
    };
    assert_eq!(decode("fingerprint='ffffffffffffffff'").unwrap(), expected);
    assert_eq!(
        decode("fingerprint_lo=4294967295,fingerprint_hi=4294967295").unwrap(),
        expected
    );
    for fields in [
        "fingerprint='0000000000000000'",
        "fingerprint='fffffffffffffffff'",
        "fingerprint='fffffffffffffffz'",
        "fingerprint='1'",
        "fingerprint=42",
        "fingerprint='ffffffffffffffff',fingerprint_lo=1",
    ] {
        assert!(decode(fields).is_err(), "accepted {fields}");
    }
}
