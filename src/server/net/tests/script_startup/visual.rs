//! UI-free downloaded visual callback over a real nonblocking listener.
use super::*;

#[test]
fn ui_free_replica_visual_worker_is_session_scoped_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.package("demo", "", "return function(_) end");
    let dir = fixture.0.join("packages/demo");
    std::fs::create_dir(dir.join("server")).unwrap();
    std::fs::create_dir(dir.join("client")).unwrap();
    std::fs::rename(dir.join("main.luau"), dir.join("server/main.luau")).unwrap();
    std::fs::write(dir.join("package.txt"), "format 2\npackage demo\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule client client_startup client/client_startup.luau\nmodule client visual client/visual.luau\n").unwrap();
    std::fs::write(
        dir.join("client/client_startup.luau"),
        "return function(h) h.set_replica_handler('demo:visual') end",
    )
    .unwrap();
    std::fs::write(dir.join("client/visual.luau"), "return function(input) if input.event ~= 'replica:entities' then return {} end local e=input.entities[1]; if not e then assert(#input.entered == 0 and #input.left == 1); return {} end assert(#input.entered == 1 and #input.left == 0 and input.entered[1].id_lo == e.id_lo and input.entered[1].id_hi == e.id_hi); return {{op='visual',id_lo=e.id_lo,id_hi=e.id_hi,yaw=0.25,bob=0.1,squash=0},{op='ember',id_lo=e.id_lo,id_hi=e.id_hi,x=0,y=0.5,z=0}} end").unwrap();
    let state = Box::new(fixture.open().unwrap());
    let fingerprint = state.world.catalog().fingerprint();
    gameplay::serve(state, |address| {
        for profile in [0xa405, 0xa406] {
            crate::client::connect_visual_probe(&address.to_string(), profile, |visual| {
                assert_eq!(visual.owner(), "demo");
                let id = (1u64 << 53) + 11;
                visual.entities(
                    vec![crate::client::presentation::EntityView {
                        id,
                        key: "demo:sproutling".into(),
                        position: [3.5, 80.0, 2.5],
                    }],
                    1,
                );
                visual.wait_for_test().unwrap();
                assert_eq!(visual.visual_pose(id), Some([0.25, 0.1, 0.0]));
                let avatar = crate::render::VisualAvatar {
                    animation: Default::default(),
                    model: crate::render::AvatarModel::Registered(
                        crate::content::MOSSBUN_ENTITY_TYPE,
                    ),
                    pose: [0.0; 4],
                    airborne: false,
                    id,
                    position: glam::Vec3::new(3.5, 80.0, 2.5),
                    cosmetics: [0; 4],
                    light_levels: [0; 4],
                    bounce: [0; 4],
                };
                let embers = visual.effects(std::time::Instant::now(), &[avatar]);
                assert_eq!(embers.len(), 1);
                assert_eq!(embers[0].center, glam::Vec3::new(3.5, 80.5, 2.5));
                visual.entities(vec![], 0);
                visual.wait_for_test().unwrap();
                assert_eq!(visual.visual_pose(id), None);
                assert!(visual.effects(std::time::Instant::now(), &[]).is_empty());
            })
            .unwrap();
        }
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
}
