use super::*;
#[test]
fn draft_cancel_apply_echo_and_duplicate_apply_are_distinct() {
    let mut editor = CharacterEditor::default();
    editor.open(state(None));
    let recipe = Some(CharacterRecipe {
        body: 1,
        hair_color: [1, 128, 255],
        ..Default::default()
    });
    editor.edit(recipe);
    assert!(editor.panel().can_apply);
    editor.open(state(None));
    assert_eq!(
        editor.panel().recipe,
        Some(CharacterRecipe::default()),
        "reopen discards unapplied draft"
    );
    editor.edit(recipe);
    assert_eq!(editor.apply(), Some(recipe));
    assert_eq!(editor.apply(), None);
    assert!(editor.panel().pending);
    editor.observe(state(None));
    assert!(editor.panel().pending);
    editor.open(state(None));
    assert!(
        editor.panel().pending,
        "closing cannot undo an in-flight request"
    );
    editor.observe(state(recipe));
    assert!(!editor.panel().pending);
    assert_eq!(editor.panel().status, "Saved on this server");
    assert_eq!(
        editor.apply(),
        None,
        "unchanged appearance is not resubmitted"
    );
}
#[test]
fn unknown_snapshot_and_rejected_draft_cannot_apply_and_disconnect_clears_state() {
    let mut editor = CharacterEditor::default();
    editor.open(None);
    editor.edit(Some(CharacterRecipe::default()));
    assert_eq!(editor.apply(), None);
    editor.observe(state(None));
    editor.edit(Some(CharacterRecipe {
        hair: 255,
        ..Default::default()
    }));
    assert_eq!(editor.apply(), None);
    editor.edit(Some(CharacterRecipe {
        body: 1,
        ..Default::default()
    }));
    assert!(editor.apply().is_some());
    editor = CharacterEditor::default();
    assert!(!editor.panel().pending);
    assert!(!editor.panel().can_apply);
    assert_ne!(editor.panel().status, "Saved on this server");
}

fn state(character: Option<CharacterRecipe>) -> Option<AppearanceState> {
    Some(AppearanceState {
        palettes: [2, 4, 1],
        character,
    })
}

#[test]
fn default_articulated_preview_preserves_palettes_and_clean_drafts_follow_server_changes() {
    let mut editor = CharacterEditor::default();
    editor.open(state(None));
    assert_eq!(editor.panel().cosmetics, [2, 4, 1, 0]);
    let first = Some(CharacterRecipe {
        body: 1,
        ..Default::default()
    });
    editor.observe(state(first));
    assert_eq!(editor.panel().recipe, first);
    let draft = Some(CharacterRecipe {
        hair: 0,
        ..Default::default()
    });
    editor.edit(draft);
    editor.observe(state(None));
    assert_eq!(
        editor.panel().recipe,
        draft,
        "authoritative change must not erase an unapplied draft"
    );
    assert!(editor.panel().can_apply);
}

#[test]
fn implicit_default_is_editable_without_a_model_toggle_or_phantom_changes() {
    let mut editor = CharacterEditor::default();
    editor.open(state(None));
    assert_eq!(editor.panel().recipe, Some(CharacterRecipe::default()));
    assert!(!editor.panel().can_apply);
    editor.edit(None);
    assert_eq!(editor.apply(), None);
    editor.observe(state(Some(CharacterRecipe::default())));
    assert!(!editor.panel().can_apply);
}

#[test]
fn preview_run_selection_is_bounded_and_does_not_edit_recipe() {
    let mut editor = CharacterEditor::default();
    let recipe = editor.draft;
    editor.clip(5);
    assert_eq!(editor.panel().clip, 5);
    editor.clip(6);
    assert_eq!(editor.panel().clip, 5);
    assert_eq!(editor.draft, recipe);
}
