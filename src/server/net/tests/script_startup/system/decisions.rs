//! Owner edits and Luau removal decisions share one retryable WAL admission.
use super::*;

const REGISTER: &str = "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,edit_cause='burn',seeds={{x=0,y=5,z=0,data='new'}}}; h.register_handler('demo:removed',1,'BlockRemoved','bloxgloom:stone','demo:action') end";
const OWNER: &str = "return function(c) local block=c.block_info(2,80,0); assert(block.state=='bloxgloom:stone'); c.edit(2,80,0,block.state,'bloxgloom:air'); return 'done',10000 end";

fn package(fixture: &Fixture, handler: &str) {
    fixture.system(REGISTER, OWNER);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:actions/v1\nmodule action action.luau\n"),
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/action.luau"), handler).unwrap();
}

#[test]
fn luau_burn_removal_context_and_drop_commit_with_owner_receipt() {
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(c,e) assert(e.kind=='BlockRemoved' and e.cause=='Burn'); assert(e.previous.state=='bloxgloom:stone' and e.previous.block_type=='bloxgloom:stone'); assert(e.cell[1]==2 and e.cell[2]==80 and e.cell[3]==0); assert(type(e.random_lo)=='number' and type(e.random_hi)=='number'); assert(c.block(2,80,0).state=='bloxgloom:air'); c.spawn_drop(2.5,80.5,0.5,'bloxgloom:stick',1,0) end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.world.edit(2, 80, 0, STONE).unwrap();
    state.world.get_chunk(KEY).unwrap();
    let (wave, missing) = stage(&mut state, "demo:clock", 1).unwrap();
    assert!(missing.is_empty());
    assert_eq!(state.world.cached_block(2, 80, 0), Some(STONE));
    assert!(crate::server::drops::nearby(&state.entities, [2.5, 80.5, 0.5]).is_empty());
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    assert_eq!(value(&state, "demo:clock"), (1, b"done".to_vec()));
    let drops = crate::server::drops::nearby(&state.entities, [2.5, 80.5, 0.5]);
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].count, 1);
    let fingerprint = state.world.catalog().fingerprint();
    super::super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x549).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
    });
    let mut restored = fixture.open().unwrap();
    assert_eq!(restored.world.get_block(2, 80, 0).unwrap(), AIR);
    assert_eq!(value(&restored, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(
        crate::server::drops::nearby(&restored.entities, [2.5, 80.5, 0.5]).len(),
        1
    );
}

#[test]
fn caught_invalid_burn_removal_decision_rejects_whole_owner_wave() {
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(c,e) assert(e.cause=='Burn'); pcall(function() c.spawn_drop(2.5,80.5,0.5,'bloxgloom:stick',129,0) end) end",
    );
    let mut state = fixture.open().unwrap();
    state.world.edit(2, 80, 0, STONE).unwrap();
    state.world.get_chunk(KEY).unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(state.world.cached_block(2, 80, 0), Some(STONE));
    assert_eq!(value(&state, "demo:clock"), (0, b"new".to_vec()));
    assert!(crate::server::drops::nearby(&state.entities, [2.5, 80.5, 0.5]).is_empty());
}
