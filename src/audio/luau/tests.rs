use super::*;
#[test]
fn routing_defaults_and_unknown_routes_are_checked() {
    let lua = mlua::Lua::new();
    let table = lua
        .load("return {kind='play',voice='beep',clip='demo:beep',position={0,0,0}}")
        .eval::<Table>()
        .unwrap();
    assert!(matches!(
        decode(&table).unwrap().1,
        Kind::Play {
            bus: bloxgloom_host_api::sound::Bus::Effects,
            ..
        }
    ));
    for (name, bus) in [("ambient", 0), ("effects", 1), ("ui", 2), ("music", 3)] {
        table.raw_set("bus", name).unwrap();
        assert!(
            matches!(decode(&table).unwrap().1, Kind::Play { bus: route, .. } if route as u8==bus)
        );
    }
    table.raw_set("bus", "master").unwrap();
    assert!(decode(&table).is_err());
    table.raw_set("bus", 1).unwrap();
    assert!(decode(&table).is_err());
}
