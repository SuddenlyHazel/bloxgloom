use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-lighting-model-bundle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("demo");
        fs::create_dir_all(dir.join("server")).unwrap();
        fs::create_dir_all(dir.join("assets/models")).unwrap();
        fs::create_dir_all(dir.join("assets/textures")).unwrap();
        fs::write(
            dir.join("assets/models/model.glb"),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fixtures/authored-model/model.glb"
            )),
        )
        .unwrap();
        fs::write(
            dir.join("assets/models/controls.json"),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fixtures/authored-model/controls.json"
            )),
        )
        .unwrap();
        let file = fs::File::create(dir.join("assets/textures/leaf.png")).unwrap();
        let mut encoder = png::Encoder::new(file, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[120, 180, 60, 255].repeat(256))
            .unwrap();
        fs::write(
            dir.join("package.txt"),
            "format 2\npackage demo\nversion 1.0.0\nentry main\n\
             requires bloxgloom:content/v1\n\
             requires bloxgloom:actions/v1\n\
             requires bloxgloom:moving_entities/v1\n\
             requires bloxgloom:mobile_entities/v1\n\
             module server main server/main.luau\n\
             module server body server/body.luau\n\
             asset model rig assets/models/model.glb\n\
             asset model-controls looks assets/models/controls.json\n\
             asset texture leaf assets/textures/leaf.png\n",
        )
        .unwrap();
        fs::write(dir.join("server/body.luau"), "return function(c,e) end").unwrap();
        fs::write(
            dir.join("server/main.luau"),
            r#"return function(h)
                h.register_texture('demo:leaf','leaf',{
                    alpha_cutout=true,foliage_wrap=0.35,foliage_transmission=0.28
                })
                h.register_block('demo:leaf','Leaf','demo:leaf',{
                    material='cutout',solid=false,sky_attenuation=2
                })
                h.register_player_model {
                    key='demo:player',asset='demo:rig',controls='demo:looks',
                    clips={idle='idle',walk='bounce',run='bounce',crouch='nod'},
                    first_person_hide={'eyes_classic'},first_person_offset={0,0,-0.2}
                }
                h.register_model {key='demo:creature',asset='demo:rig',controls='demo:looks'}
                h.register_creature {
                    key='demo:sprout',module='demo:body',schema=1,revision=1,
                    max_state_bytes=8,initial_state='',interval=10,read_radius=1,
                    body={half_width=0.36,height=1.9,speed=1},
                    model={key='demo:creature',idle='idle',walk='bounce',run='bounce'}
                }
                h.register_moving_entity {
                    key='demo:crate',module='demo:body',schema=1,revision=1,
                    max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=3000,
                    body={half_extents={0.3,0.15,0.1},max_speed=16,max_acceleration=32,
                        gravity_scale=1,response='bounce',restitution=0.35},
                    physics={linear_damping=0.1,angular_damping=0.5,friction=0.7,max_angular_speed=4},
                    model={{min={-0.3,-0.15,-0.1},max={0.3,0.15,0.1},color={0.3,0.7,0.9}}}
                }
            end"#,
        )
        .unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn lighting_wraps_player_models_rigid_bodies_and_authored_creatures_without_version_collisions() {
    let fixture = Fixture::new();
    let declarations = crate::server::script::startup::Declarations::discover(&fixture.0).unwrap();
    let bundle = &declarations.client_bundle;
    let mut inner = bundle.bytes();
    for magic in [
        MAGIC,
        super::super::creature_authored::MAGIC,
        super::super::moving::MAGIC,
        super::super::models::MAGIC,
    ] {
        assert!(inner.starts_with(magic));
        inner = Reader(&inner[magic.len()..])
            .field(MAX_BUNDLE_BYTES)
            .unwrap();
    }

    let verified = ClientBundle::decode_verify(bundle.bytes(), bundle.cache_key()).unwrap();
    let server =
        crate::server::catalog_with_extension(crate::content::Catalog::builtins(), &declarations)
            .unwrap();
    let client = verified.session_catalog().unwrap();
    assert_eq!(server.fingerprint(), client.fingerprint());
    let model = client
        .player_model(client.player_model_id("demo:player").unwrap())
        .unwrap();
    let player = model.player.as_ref().unwrap();
    assert_eq!(player.walk.as_deref(), Some("bounce"));
    assert_eq!(player.first_person_hide, ["eyes_classic"]);
    assert_eq!(player.first_person_offset, [0.0, 0.0, -0.2]);
    assert_eq!(model.model.controls.variants[0].name, "eyes");
    assert_eq!(model.model.controls.layers[1].name, "hat");
    assert_eq!(model.model.controls.tints[1].name, "iris");
    let creature = client
        .mobile_entity(client.entity_type_id_by_key("demo:sprout").unwrap())
        .unwrap();
    assert_eq!(
        creature.authored_model.as_ref().unwrap().key,
        "demo:creature"
    );
    let moving = client
        .moving_entity(client.entity_type_id_by_key("demo:crate").unwrap())
        .unwrap();
    assert_eq!(moving.physics.unwrap().friction, 0.7);
    let texture = client
        .textures()
        .iter()
        .find(|t| t.key == "demo:leaf")
        .unwrap();
    assert_eq!(
        texture.foliage,
        FoliageShading {
            wrap: 0.35,
            transmission: 0.28
        }
    );
    assert_eq!(
        client.sky_attenuation(client.state_by_key("demo:leaf").unwrap()),
        2
    );
}

#[test]
fn foliage_codec_rejects_nonfinite_out_of_range_and_redundant_metadata() {
    for (a, b, valid) in [
        (0.35, 0.28, true),
        (0.0, 1.0, true),
        (0.0, 0.0, false),
        (-0.0, 0.5, false),
        (-0.1, 0.5, false),
        (0.5, 1.1, false),
        (f32::NAN, 0.2, false),
        (0.2, f32::INFINITY, false),
    ] {
        let bytes = [a.to_le_bytes(), b.to_le_bytes()].concat();
        assert_eq!(shading(&bytes).is_ok(), valid);
    }
    assert!(shading(&[0; 7]).is_err());
    assert!(shading(&[0; 9]).is_err());
}
