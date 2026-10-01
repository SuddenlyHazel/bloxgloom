use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(startup: &str, capabilities: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-moving-startup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let package = root.join("throw");
        fs::create_dir_all(package.join("server")).unwrap();
        let requires = if capabilities {
            "requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:moving_entities/v1\n"
        } else {
            ""
        };
        fs::write(package.join("package.txt"), format!("format 2\npackage throw\nversion 1.0.0\nentry main\n{requires}module server main server/main.luau\nmodule server behavior server/behavior.luau\n")).unwrap();
        fs::write(
            package.join("server/main.luau"),
            format!("return function(h) {startup} end"),
        )
        .unwrap();
        fs::write(
            package.join("server/behavior.luau"),
            "return function(c,e) end",
        )
        .unwrap();
        Self(root)
    }
    fn discover(&self) -> std::io::Result<Declarations> {
        Declarations::discover(&self.0)
    }
    fn error(&self) -> String {
        self.discover().err().expect("startup rejected").to_string()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn declaration() -> &'static str {
    "{key='throw:seed',module='throw:behavior',schema=1,revision=1,max_state_bytes=8,max_public_bytes=1,interval=2,lifetime_ticks=100,handles_impact=true,handles_expiry=true,body={half_extents={0.1,0.1,0.1},max_speed=16,max_acceleration=32,response='bounce',restitution=0.8},model={{min={-0.1,-0.1,-0.1},max={0.1,0.1,0.1},color={0.3,0.6,0.1}}}}"
}

#[test]
fn moving_startup_collects_frozen_body_and_three_handlers() {
    let fixture = Fixture::new(&format!("h.register_moving_entity{}", declaration()), true);
    let declarations = fixture.discover().unwrap();
    assert_eq!(declarations.moving.len(), 1);
    let entity = &declarations.moving[0];
    assert_eq!(entity.body.response, Response::Bounce);
    assert_eq!(entity.body.half_extents, [0.1; 3]);
    assert_eq!(entity.model.len(), 1);
    assert_eq!(entity.source_exclusion_ticks, 0);
    assert_eq!(declarations.handlers.len(), 3);
    assert!(
        declarations
            .handlers
            .iter()
            .all(|h| h.target.as_deref() == Some("throw:seed"))
    );
    entity.state.validate(&[0; 8]).unwrap();
    assert!(entity.state.validate(&[0; 7]).is_err());
    assert_eq!(entity.state.public(&[1; 8]).unwrap(), [1]);
}

#[test]
fn moving_startup_rejects_capability_and_caught_invalid_declarations_atomically() {
    let startup = format!("h.register_moving_entity{}", declaration());
    assert!(Fixture::new(&startup, false).error().contains("requires"));
    let caught = format!(
        "local d={};d.body.half_extents={{0.1,0.1,0.1,extra=1}};pcall(function() h.register_moving_entity(d) end);h.register_item('throw:token','Token','bloxgloom:stone')",
        declaration()
    );
    assert!(Fixture::new(&caught, true).error().contains("dense"));
    for edit in [
        "d.body.origin='feet'",
        "d.body.max_speed=65",
        "d.body.max_acceleration=0/0",
        "d.source_exclusion_ticks=21",
        "d.module='throw:missing'",
        "d.key='other:seed'",
        "setmetatable(d,{})",
        "d.model[1].motion='left_foot'",
    ] {
        let startup = format!(
            "local d={};{edit};h.register_moving_entity(d)",
            declaration()
        );
        assert!(Fixture::new(&startup, true).discover().is_err(), "{edit}");
    }
}

#[test]
fn moving_startup_bounds_registration_and_handler_counts() {
    let startup = format!(
        "for i=1,9 do local d={};d.key='throw:seed'..i;h.register_moving_entity(d) end",
        declaration()
    );
    assert!(
        Fixture::new(&startup, true)
            .error()
            .contains("limit exceeded (8)")
    );
    let startup = format!(
        "h.register_moving_entity{};h.register_moving_entity{}",
        declaration(),
        declaration()
    );
    assert!(Fixture::new(&startup, true).error().contains("duplicate"));
}

#[test]
fn moving_startup_supports_private_model_free_entities() {
    let startup = format!(
        "local d={};d.max_public_bytes=0;d.model=nil;d.handles_impact=false;d.handles_expiry=false;h.register_moving_entity(d)",
        declaration()
    );
    let fixture = Fixture::new(&startup, true);
    let declarations = fixture.discover().unwrap();
    assert_eq!(declarations.handlers.len(), 1);
    assert!(declarations.moving[0].model.is_empty());
    assert!(declarations.moving[0].state.public(&[0; 8]).unwrap().is_empty());
}
