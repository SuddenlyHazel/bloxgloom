use super::*;
use bloxgloom_host_api::gameplay::{Weather, WeatherKind, WorldTime};
use std::collections::BTreeSet;
struct Content(BTreeSet<String>);
impl Tags for Content {
    fn members(&self, kind: TagKind, key: &str) -> Option<&BTreeSet<String>> {
        (kind == TagKind::Block && key == "test:logs").then_some(&self.0)
    }
}
fn environment() -> Environment {
    Environment {
        world_time: WorldTime {
            elapsed_ms: 1234,
            cycle_ms: 9000,
        },
        weather: Weather {
            kind: WeatherKind::Rain,
            revision: 7,
            elapsed_ms: 1234,
            rain_mm_h: 2.0,
            wind_m_s: 1.0,
            cloud: 0.5,
            transition: 1.0,
        },
    }
}
#[test]
fn captured_reads_are_readonly_sorted_and_paginated() {
    let lua = Lua::new();
    let host = lua.create_table().unwrap();
    let content = Content(["test:oak".into(), "test:birch".into()].into());
    lua.globals().set("host", host.clone()).unwrap();
    with(&lua, &host, Some(environment()), Some(&content), || {
        lua.load(
            r#"
        assert(host.tag_contains("block", "test:logs", "test:oak"))
        assert(not host.tag_contains("block", "test:logs", "test:stone"))
        local members, total = host.tag_members("block", "test:logs", 1, 1)
        assert(total == 2 and #members == 1 and members[1] == "test:oak")
        assert(not pcall(function() members[1] = "bad" end))
        assert(host.world_time().elapsed_ms == 1234)
        assert(host.weather().kind == "rain")
        retained = host.tag_contains
    "#,
        )
        .exec()
    })
    .unwrap();
    assert!(
        lua.load("retained('block', 'test:logs', 'test:oak')")
            .exec()
            .is_err()
    );
}
#[test]
fn caught_unknown_tags_and_excess_queries_reject_callback() {
    let lua = Lua::new();
    let host = lua.create_table().unwrap();
    let content = Content(["test:oak".into()].into());
    lua.globals().set("host", host.clone()).unwrap();
    assert!(
        with(&lua, &host, None, Some(&content), || lua
            .load(
                r#"
        pcall(function() host.tag_contains('block', 'test:missing', 'test:oak') end)
    "#
            )
            .exec())
        .is_err()
    );
    assert!(
        with(&lua, &host, Some(environment()), Some(&content), || lua
            .load(
                r#"
        for i = 1, 65 do pcall(function() host.weather() end) end
    "#
            )
            .exec())
        .is_err()
    );
}
