use super::*;
use winit::{dpi::PhysicalPosition, event::DeviceId, event::MouseButton};

#[test]
fn menu_state_tracks_cursor_and_release_through_a_gameplay_interval() {
    let device_id = DeviceId::dummy();
    // Hold a menu button, press Escape to resume before releasing it, move the
    // cursor during play, then reopen a menu without an extra mouse movement.
    let events = [
        WindowEvent::Focused(true),
        WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(120.0, 240.0),
        },
        WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: MouseButton::Left,
        },
        WindowEvent::MouseInput {
            device_id,
            state: ElementState::Released,
            button: MouseButton::Left,
        },
        WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(320.0, 180.0),
        },
        WindowEvent::Focused(false),
        WindowEvent::Focused(true),
    ];
    let retained: Vec<_> = events
        .iter()
        .filter(|event| is_state_event(event))
        .collect();
    assert_eq!(retained.len(), 6);
    assert!(retained.iter().any(|event| matches!(
        event,
        WindowEvent::MouseInput {
            state: ElementState::Released,
            ..
        }
    )));
    assert!(
        matches!(retained[3], WindowEvent::CursorMoved { position, .. }
        if position.x == 320.0 && position.y == 180.0)
    );
    assert!(matches!(retained[4], WindowEvent::Focused(false)));
    assert!(matches!(retained[5], WindowEvent::Focused(true)));
}

#[test]
fn gameplay_mouse_presses_stay_out_of_egui_but_every_button_release_clears_state() {
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        let event = |state| WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button,
        };
        assert!(!is_state_event(&event(ElementState::Pressed)));
        assert!(is_state_event(&event(ElementState::Released)));
    }
}
