//! Independent finite item-use proof over the host's atomic inventory operation.
use bloxgloom_host_api::{Registrar, RegistrationError, actions::*};
pub const KEY: &str = "fixture:knap";
pub fn definition() -> Action {
    Action {
        key: KEY.into(),
        version: 1,
        label: "KNAP GRAVEL".into(),
        command: None,
        target: Target::Item("bloxgloom:gravel".into()),
        operation: Operation::Recipe {
            input: "bloxgloom:gravel".into(),
            consume: 2,
            output: "bloxgloom:stick".into(),
            produce: 3,
        },
        panel: Some(Panel {
            title: "FIELD KNAPPING".into(),
            widgets: vec![
                Widget::Label("Turn two gravel into three sticks.".into()),
                Widget::Label("Only plain stacks can be worked.".into()),
                Widget::Button {
                    action: Some(KEY.into()),
                    label: "KNAP TWO GRAVEL".into(),
                    tooltip: "Consumes 2 gravel. Requires space for 3 sticks.".into(),
                },
            ],
        }),
    }
}
pub fn register(r: &mut dyn Registrar) -> Result<(), RegistrationError> {
    r.action(definition())
}
