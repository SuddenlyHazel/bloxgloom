//! Native double-tap input and late-join locomotion state over the real listener.
use super::*;

#[test]
fn sprint_native_input_reconciles_speed_and_replicates_start_stop_crouch_and_disconnect() {
    let fixture = Fixture::new();
    let mut state = Box::new(crate::server::server_state(7, fixture.0.join("save")).unwrap());
    state.admin_profile = Some(0x5c72);
    state.spawn_anchor = [0.5, 85.0, 0.5];
    for x in -3..=3 {
        for z in -3..=3 {
            for y in 84..=90 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 84 {
                            crate::world::STONE
                        } else {
                            crate::world::AIR
                        },
                    )
                    .unwrap();
            }
        }
    }
    gameplay::serve(state, |address| {
        crate::client::exercise_player_sprint(&address.to_string(), fixture.0.join("client.cfg"));
    });
}
