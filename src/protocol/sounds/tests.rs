use super::*;
#[test]
fn unknown_bus_rejects_the_complete_wire_batch() {
    let event = Event {
        owner: "demo".into(),
        voice: "tone".into(),
        kind: Kind::Play {
            bus: bloxgloom_host_api::sound::Bus::Music,
            clip: "demo:tone".into(),
            position: [0.0; 3],
            entity: None,
            gain: 1.0,
            pitch: 1.0,
            looping: false,
        },
    };
    let mut bytes = vec![0, 0];
    write(&mut bytes, 1, std::slice::from_ref(&event)).unwrap();
    assert_eq!(bytes.len() - 2, len(std::slice::from_ref(&event)));
    assert_eq!(read(&mut Cursor::new(&bytes)).unwrap().1, [event]);
    // Two header bytes, batch ID/count, two short strings and operation tag.
    let bus_offset = 2 + 9 + (1 + 4) + (1 + 4) + 1;
    bytes[bus_offset] = 255;
    assert!(read(&mut Cursor::new(&bytes)).is_err());
}
