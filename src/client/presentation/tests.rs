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
