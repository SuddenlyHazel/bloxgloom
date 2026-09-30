//! Actual client key dispatch queues declared events onto its UI worker.
use super::*;

fn app(scope: &str, open: bool) -> (ClientApp, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "bloxgloom-input-bindings-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let package = directory.join("packages/inputdemo");
    std::fs::create_dir_all(package.join("server")).unwrap();
    std::fs::create_dir_all(package.join("client")).unwrap();
    std::fs::create_dir_all(package.join("assets/ui")).unwrap();
    std::fs::create_dir_all(package.join("assets/fonts")).unwrap();
    std::fs::write(package.join("package.txt"),"format 2\npackage inputdemo\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule client view client/view.luau\nasset ui-document page assets/ui/page.json\nasset ui-style panel assets/ui/panel.json\nasset ui-style label assets/ui/label.json\nasset ui-style field assets/ui/field.json\nasset ui-font body assets/fonts/body.ttf\n").unwrap();
    std::fs::write(package.join("server/main.luau"), "return function(_) end").unwrap();
    std::fs::write(package.join("client/view.luau"),"return function(input) assert(input.event == 'inputdemo:toggle' and input.value == 'pressed'); return {{op='state',value=input.state .. 'x'},{op='text',node='inputdemo:page/count',value=tostring(#input.state+1)}} end").unwrap();
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages/uidemo/assets");
    for file in [
        "ui/panel.json",
        "ui/label.json",
        "ui/field.json",
        "fonts/body.ttf",
    ] {
        let target = package.join("assets").join(file);
        if file.ends_with(".json") {
            let text = std::fs::read_to_string(fixture.join(file))
                .unwrap()
                .replace("uidemo:", "inputdemo:");
            std::fs::write(target, text).unwrap();
        } else {
            std::fs::copy(fixture.join(file), target).unwrap();
        }
    }
    std::fs::write(package.join("assets/ui/page.json"),format!(r#"{{"version":2,"presentation":{{"capability":"local-ui","module":"inputdemo:view"}},"bindings":[{{"key":"inputdemo:toggle","event":"inputdemo:toggle","default":"B","scope":"{scope}","open":{open}}}],"nodes":[{{"id":"root","kind":"panel","style":"inputdemo:panel"}},{{"id":"count","parent":0,"kind":"label","style":"inputdemo:label","text":"0"}},{{"id":"field","parent":0,"kind":"input","style":"inputdemo:field","text":""}}]}}"#)).unwrap();
    let snapshot = crate::server::PackageSnapshot::discover(
        &directory.join("packages").canonicalize().unwrap(),
    )
    .unwrap();
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        directory.join("config"),
    );
    app.package_ui = Some(crate::ui::authored::Session::new(Arc::clone(
        snapshot.client_bundle().ui().unwrap(),
    )));
    (app, directory)
}

#[test]
fn declared_input_real_client_queues_once_opens_and_rebinds_without_gameplay() {
    let (mut app, directory) = app("both", true);
    assert!(app.package_binding_key(KeyCode::KeyB, false, false));
    assert_eq!(app.screen, UiScreen::Package);
    assert!(app.package_binding_key(KeyCode::KeyB, true, false));
    app.package_ui
        .as_mut()
        .unwrap()
        .wait_for_presentation()
        .unwrap();
    assert_eq!(app.package_ui.as_ref().unwrap().text_at(1), "1");
    assert!(app.pending_commands.is_empty());
    assert!(
        app.config
            .named_bindings
            .bind("inputdemo:toggle", KeyCode::KeyT, app.config.bindings)
    );
    assert!(!app.package_binding_key(KeyCode::KeyB, false, false));
    assert!(app.package_binding_key(KeyCode::KeyT, false, false));
    app.package_ui
        .as_mut()
        .unwrap()
        .wait_for_presentation()
        .unwrap();
    assert_eq!(app.package_ui.as_ref().unwrap().text_at(1), "2");
    app.config_writer.finish();
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn declared_input_real_client_respects_screens_focus_modifiers_and_scope() {
    let (mut app, directory) = app("game", false);
    for screen in [
        UiScreen::Admin,
        UiScreen::Inventory,
        UiScreen::Settings,
        UiScreen::Package,
    ] {
        app.set_screen(screen);
        assert!(!app.package_binding_key(KeyCode::KeyB, false, false));
    }
    app.set_screen(UiScreen::Playing);
    assert!(!app.package_binding_key(KeyCode::KeyB, false, true));
    app.input_modifiers = winit::keyboard::ModifiersState::CONTROL;
    assert!(!app.package_binding_key(KeyCode::KeyB, false, false));
    app.input_modifiers = winit::keyboard::ModifiersState::empty();
    assert!(app.package_binding_key(KeyCode::KeyB, false, false));
    app.package_ui
        .as_mut()
        .unwrap()
        .wait_for_presentation()
        .unwrap();
    assert_eq!(app.screen, UiScreen::Playing);
    assert_eq!(app.package_ui.as_ref().unwrap().text_at(1), "1");
    app.config_writer.finish();
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();

    let (mut app, directory) = self::app("ui", false);
    assert!(!app.package_binding_key(KeyCode::KeyB, false, false));
    app.set_screen(UiScreen::Package);
    app.package_ui
        .as_mut()
        .unwrap()
        .apply_egui(crate::ui::authored::EguiIntent::Focus(2));
    assert!(
        !app.package_binding_key(KeyCode::KeyB, false, false),
        "focused input must retain letter keys"
    );
    app.config_writer.finish();
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}
