//! Window state must reach egui even when gameplay owns active input.
use winit::event::{ElementState, WindowEvent};

pub(super) fn is_state_event(event: &WindowEvent) -> bool {
    if let WindowEvent::KeyboardInput { event, .. } = event {
        return event.state == ElementState::Released;
    }
    matches!(
        event,
        WindowEvent::Focused(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Resized(_)
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::MouseInput {
                state: ElementState::Released,
                ..
            }
    )
}

#[cfg(test)]
mod tests;
