//! Exact identity of the installed native GLB and its public appearance choices.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
pub(super) fn fingerprint() -> &'static [u8; 32] {
    static HASH: OnceLock<[u8; 32]> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash=Sha256::new();
        hash.update(b"bloxgloom-character-recipe/v3;native-glb-master/v1;authored-tracks/v1;embedded-only/v1;basis=y-up,+z,1.0;rigid-player/v1");
        hash.update(super::CharacterRecipe::default().encode());
        for (group,names) in [("bodies",super::BODIES.as_slice()),("eyes",super::EYES.as_slice()),("mouths",super::MOUTHS.as_slice()),("hair",super::HAIR.as_slice())] {
            hash.update(group.as_bytes());
            for name in names { hash.update((name.len() as u64).to_le_bytes()); hash.update(name.as_bytes()); }
        }
        for (name,bytes) in [("master/model.glb",include_bytes!("../../assets/models/player/master/model.glb").as_slice()),("master/controls.json",include_bytes!("../../assets/models/player/master/controls.json").as_slice())] {
            hash.update(name.as_bytes()); hash.update((bytes.len() as u64).to_le_bytes()); hash.update(bytes);
        }
        hash.finalize().into()
    })
}
