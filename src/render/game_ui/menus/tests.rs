use super::*;

#[test]
fn production_egui_graphics_exposes_and_labels_authored_character_control() {
    for authored in [false, true] {
        let settings = crate::ui::UiSettings {
            authored_characters: authored,
            ..Default::default()
        };
        let row = settings_rows(settings, true)
            .into_iter()
            .find(|row| row.0 == SettingId::Characters)
            .expect("live egui needs the character toggle, not only the legacy preview UI");
        assert_eq!(row.1, "Characters");
        assert_eq!(row.2, if authored { "Authored" } else { "Classic" });
    }
}
