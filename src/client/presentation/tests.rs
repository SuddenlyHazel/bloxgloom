//! Malformed downloaded appearance commands fail before reaching the window thread.
use super::*;

#[test]
fn authored_action_arguments_are_exact_bytes_and_bounded() {
    let request = |arguments: &str| Request {
        script: Arc::new(Script {
            module: "demo:ui".into(),
            source: format!(
                "return function(_) return {{{{op='action',key='demo:use',arguments={arguments}}}}} end"
            ),
        }),
        sequence: 1,
        event: "demo:click".into(),
        value: String::new(),
        state: String::new(),
        texts: vec![],
        node: None,
        values: vec![],
        replica: false,
        observations: Arc::new(Observations::default()),
        entities: vec![],
        entered: vec![],
        left: vec![],
    };
    assert!(matches!(
        run(request("string.char(0, 255, 1)")).unwrap().as_slice(),
        [Command::Action(key, bytes)] if key == "demo:use" && bytes == &[0, 255, 1]
    ));
    assert!(run(request("string.rep('x', 131)")).is_err());
    assert!(run(request("42")).is_err());
}

#[test]
fn tint_commands_reject_nonfinite_and_out_of_range_channels() {
    for color in ["r=0/0,g=1,b=1", "r=-0.1,g=1,b=1", "r=1,g=1.1,b=1"] {
        let request = Request {
            script: Arc::new(Script {
                module: "demo:visual".into(),
                source: format!(
                    "return function(_) return {{{{op='tint',id_lo=1,id_hi=0,{color}}}}} end"
                ),
            }),
            sequence: 1,
            event: "replica:entities".into(),
            value: String::new(),
            state: String::new(),
            texts: vec![],
            node: None,
            values: vec![],
            replica: true,
            observations: Arc::new(Observations::default()),
            entities: vec![],
            entered: vec![],
            left: vec![],
        };
        assert!(run(request).is_err(), "{color}");
    }
}

#[test]
fn spark_commands_reject_unbounded_color_and_offset() {
    for fields in [
        "x=2,y=0,z=0,r=1,g=1,b=1",
        "x=0,y=0,z=0,r=0/0,g=1,b=1",
        "x=0,y=0,z=0,r=1,g=-0.1,b=1",
        "x=0,y=0,z=0,r=1,g=1,b=1,size=0.51",
        "x=0,y=0,z=0,r=1,g=1,b=1,size=0/0",
        "x=0,y=0,z=0,r=1,g=1,b=1,lifetime_ms=99",
        "x=0,y=0,z=0,r=1,g=1,b=1,lifetime_ms=2001",
        "x=0,y=0,z=0,r=1,g=1,b=1,lifetime_ms=100.5",
    ] {
        let request = Request {
            script: Arc::new(Script {
                module: "demo:visual".into(),
                source: format!(
                    "return function(_) return {{{{op='spark',id_lo=1,id_hi=0,{fields}}}}} end"
                ),
            }),
            sequence: 1,
            event: "replica:entities".into(),
            value: String::new(),
            state: String::new(),
            texts: vec![],
            node: None,
            values: vec![],
            replica: true,
            observations: Arc::new(Observations::default()),
            entities: vec![],
            entered: vec![],
            left: vec![],
        };
        assert!(run(request).is_err(), "{fields}");
    }
}

#[test]
fn spark_commands_preserve_defaults_and_bounded_custom_appearance() {
    for (options, expected_size, expected_life) in
        [("", 0.16, 850), (",size=0.28,lifetime_ms=1200", 0.28, 1200)]
    {
        let request = Request {
            script: Arc::new(Script {
                module: "demo:visual".into(),
                source: format!(
                    "return function(_) return {{{{op='spark',id_lo=1,id_hi=0,x=0,y=0,z=0,r=1,g=1,b=1{options}}}}} end"
                ),
            }),
            sequence: 1,
            event: "replica:anchors".into(),
            value: String::new(),
            state: String::new(),
            texts: vec![],
            node: None,
            values: vec![],
            replica: true,
            observations: Arc::new(Observations::default()),
            entities: vec![],
            entered: vec![],
            left: vec![],
        };
        assert!(
            matches!(run(request).unwrap().as_slice(), [Command::Spark(1, _, _, size, life)] if *size == expected_size && *life == expected_life)
        );
    }
}

#[test]
fn replica_commands_roundtrip_exact_handles_and_reject_forged_or_wrong_kind_values() {
    let id = u64::MAX;
    let request = |target: &str| Request {
        script: Arc::new(Script {
            module: "demo:visual".into(),
            source: format!(
                "return function(input) local e=input.entities[1]; assert(e.id == input.entered[1].id); return {{{{op='tint',entity={target},r=0.5,g=1,b=0.5}}}} end"
            ),
        }),
        sequence: 1,
        event: "replica:entities".into(),
        value: "1".into(),
        state: String::new(),
        texts: vec![],
        node: None,
        values: vec![],
        replica: true,
        observations: Arc::new(Observations::default()),
        entities: vec![EntityView {
            id,
            key: "demo:creature".into(),
            position: [0.0, 0.0, 0.0],
            revision: u64::MAX,
            motion_revision: u64::MAX - 1,
            public: vec![],
        }],
        entered: vec![id],
        left: vec![],
    };
    assert!(
        matches!(run(request("e.id")).unwrap().as_slice(), [Command::Tint(actual, color)] if *actual == id && *color == [0.5, 1.0, 0.5])
    );
    for forged in [
        "e.revision",
        "e.motion_revision",
        "tostring(e.id)",
        "{id=e.id}",
        "42",
    ] {
        assert!(run(request(forged)).is_err(), "accepted {forged}");
    }
}

#[test]
fn local_ui_callback_receives_typed_replica_without_a_replica_event() {
    let observations = Observations {
        inventory: Some(InventoryView {
            revision: u64::MAX,
            slots: (0..36).map(|slot| SlotView { slot, stack: None }).collect(),
        }),
        ..Default::default()
    };
    let request=Request{script:Arc::new(Script{module:"demo:ui".into(),source:"return function(e) assert(e.event=='demo:click'); assert(#e.replica.inventory.slots==36); assert(e.replica.inventory.revision_hi==4294967295); assert(e.replica.inventory.slots[1].stack==nil); return {} end".into()}),
        sequence:1,event:"demo:click".into(),value:String::new(),state:String::new(),texts:vec![],node:None,values:vec![],replica:false,entities:vec![],observations:Arc::new(observations),entered:vec![],left:vec![]};
    assert!(run(request).unwrap().is_empty());
}
