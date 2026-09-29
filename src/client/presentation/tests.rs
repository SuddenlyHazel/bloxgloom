//! Malformed downloaded appearance commands fail before reaching the window thread.
use super::*;

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
            replica: true,
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
            replica: true,
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
            replica: true,
            entities: vec![],
            entered: vec![],
            left: vec![],
        };
        assert!(
            matches!(run(request).unwrap().as_slice(), [Command::Spark(1, _, _, size, life)] if *size == expected_size && *life == expected_life)
        );
    }
}
