use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(startup: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-model-declarations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("demo");
        fs::create_dir_all(dir.join("assets/models")).unwrap();
        fs::create_dir_all(dir.join("server")).unwrap();
        fs::write(
            dir.join("assets/models/model.glb"),
            include_bytes!("../../../../../assets/models/player/master/model.glb"),
        )
        .unwrap();
        fs::write(
            dir.join("assets/models/controls.json"),
            include_bytes!("../../../../../assets/models/player/master/controls.json"),
        )
        .unwrap();
        fs::write(dir.join("package.txt"),"format 2\npackage demo\nversion 1.0.0\nentry main\nrequires bloxgloom:content/v1\nmodule server main server/main.luau\nasset model player assets/models/model.glb\nasset model-controls looks assets/models/controls.json\n").unwrap();
        fs::write(
            dir.join("server/main.luau"),
            format!("return function(h) {startup} end"),
        )
        .unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn packaged_glbs_are_verified_prepared_and_bound_to_handshake_identity() {
    let fixture = Fixture::new(
        "h.register_model{key='demo:player',asset='demo:player',controls='demo:looks',scale=0.5}",
    );
    let declarations = Declarations::discover(&fixture.0).unwrap();
    let bundle = &declarations.client_bundle;
    assert_eq!(&bundle.bytes()[..9], b"BGCLIENT\x30");
    let client = bundle.session_catalog().unwrap();
    let mut server = crate::content::Catalog::builtins();
    crate::server::lifecycle::Registration::install(&declarations, &mut server).unwrap();
    assert_eq!(server.fingerprint(), client.fingerprint());
    assert_eq!(client.model_by_key("demo:player").unwrap().scale, 0.5);
    let verified = crate::server::script::package::client::ClientBundle::decode_verify(
        bundle.bytes(),
        bundle.cache_key(),
    )
    .unwrap();
    assert!(
        verified
            .session_catalog()
            .unwrap()
            .model_by_key("demo:player")
            .is_some()
    );
    let mut corrupt = bundle.bytes().to_vec();
    corrupt[32] ^= 1;
    assert!(
        crate::server::script::package::client::ClientBundle::decode_verify(
            &corrupt,
            bundle.cache_key()
        )
        .is_err()
    );
}
#[test]
fn bad_model_inputs_poison_caught_startup_and_never_publish_partial_models() {
    for input in [
        "key='other:model',asset='demo:player'",
        "key='demo:model',asset='other:player'",
        "key='demo:model',asset='demo:looks'",
        "key='demo:model',asset='demo:player',controls='demo:player'",
        "key='demo:model',asset='demo:player',scale=0",
        "key='demo:model',asset='demo:player',unknown=true",
    ] {
        let fixture = Fixture::new(&format!(
            "pcall(function() h.register_model{{{input}}} end)"
        ));
        assert!(
            Declarations::discover(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("register_model"),
            "{input}"
        );
    }
}
