use super::*;
fn base(capability: bool) -> Vec<u8> {
    let mut writer = Writer(super::super::super::MAGIC.to_vec());
    writer.count(1).unwrap();
    writer.field(b"demo").unwrap();
    writer.field(b"1.0.0").unwrap();
    for _ in 0..3 {
        writer.count(0).unwrap();
    } // dependencies/source/assets
    writer.count(1).unwrap(); // metadata present
    writer.count(if capability { 2 } else { 1 }).unwrap();
    writer.field(composition::CONTENT.as_bytes()).unwrap();
    if capability {
        writer
            .field(composition::MOVING_ENTITIES.as_bytes())
            .unwrap();
    }
    for _ in 0..8 {
        writer.count(0).unwrap();
    } // items/textures/blocks + five runtime categories
    writer.0
}
fn declaration() -> MovingEntity {
    MovingEntity {
        key: "demo:bolt".into(),
        schema_version: 1,
        schema_fingerprint: u64::MAX,
        max_state_bytes: 65000,
        max_public_bytes: 4000,
        body: Body {
            half_extents: [0.1, 0.2, 0.3],
            collisions: CollisionMask {
                terrain: true,
                players: true,
                creatures: false,
            },
            response: Response::Bounce,
            restitution: 0.5,
            gravity_scale: 1.0,
            max_speed: 32.0,
            max_acceleration: 64.0,
        },
        lifetime_ticks: 100,
        interval: 2,
        source_exclusion_ticks: 3,
        handles_impact: true,
        handles_expiry: false,
        model: vec![Cuboid {
            min: [-0.1; 3],
            max: [0.1; 3],
            color: [0.5, 0.6, 0.7],
            motion: PartMotion::Body,
        }],
        state: Arc::new(InertState),
    }
}
fn artifact(inner: &[u8], count: usize) -> Vec<u8> {
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(inner).unwrap();
    writer.count(count).unwrap();
    for index in 0..count {
        let mut d = declaration();
        d.key = format!("demo:bolt{index:02}");
        encode(&mut writer, &d).unwrap();
    }
    writer.0
}
fn decode_bytes(bytes: &[u8]) -> Result<ClientBundle, ScriptError> {
    ClientBundle::decode_verify(bytes, CacheKey(Sha256::digest(bytes).into()))
}
#[test]
fn moving_metadata_preserves_native_catalog_and_has_inert_codec() {
    let inner = base(true);
    let bytes = artifact(&inner, 1);
    let bundle = decode_bytes(&bytes).unwrap();
    let actual = &bundle.declarations.as_ref().unwrap().moving[0];
    let mut native = decode_bytes(&inner).unwrap().session_catalog().unwrap();
    native.register_moving(actual.clone()).unwrap();
    let client = bundle.session_catalog().unwrap();
    assert_eq!(client.fingerprint(), native.fingerprint());
    assert_eq!(actual.body, declaration().body);
    assert_eq!(actual.schema_fingerprint, u64::MAX);
    assert!(actual.state.validate(&[]).is_err());
    assert!(actual.state.public(&[]).is_err());
}
#[test]
fn moving_metadata_rejects_missing_capability_overflow_and_nesting() {
    assert!(decode_bytes(&artifact(&base(false), 1)).is_err());
    assert!(decode_bytes(&artifact(&base(true), 9)).is_err());
    let valid = artifact(&base(true), 1);
    assert!(decode_bytes(&artifact(&valid, 1)).is_err());
    for length in 0..valid.len() {
        assert!(decode_bytes(&valid[..length]).is_err());
    }
    let mut trailing = valid;
    trailing.push(0);
    assert!(decode_bytes(&trailing).is_err());
}
