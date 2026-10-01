//! Exact builtin catalog identity, cached once for repeated catalog comparisons.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(super) fn fingerprint() -> &'static [u8; 32] {
    static HASH: OnceLock<[u8; 32]> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        hash.update(b"bloxgloom-character-recipe/v1;player-payload/v2;iris-option/v1;basis=y-up,+z,0.9;rigid-joints/v1");
        for (group, names) in [("eyes", super::EYES.as_slice()), ("mouths", super::MOUTHS.as_slice()), ("hair", super::HAIR.as_slice())] {
            hash.update(group.as_bytes());
            hash.update((names.len() as u64).to_le_bytes());
            for name in names {
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
            }
        }
        let assets: &[(&str, &[u8])] = &[
            ("character.json", include_bytes!("../../assets/models/player/character.json")),
            ("body.png", include_bytes!("../../assets/models/player/body.png")),
            ("hair.png", include_bytes!("../../assets/models/player/hair.png")),
            ("hair_undercut.png", include_bytes!("../../assets/models/player/hair_undercut.png")),
            ("face/mapping.json", include_bytes!("../../assets/models/player/face/mapping.json")),
            ("face/clean.png", include_bytes!("../../assets/models/player/face/clean.png")),
            ("face/eyes/0.png", include_bytes!("../../assets/models/player/face/eyes/0.png")),
            ("face/eyes/1.png", include_bytes!("../../assets/models/player/face/eyes/1.png")),
            ("face/eyes/2.png", include_bytes!("../../assets/models/player/face/eyes/2.png")),
            ("face/eyes/3.png", include_bytes!("../../assets/models/player/face/eyes/3.png")),
            ("face/eyes/4.png", include_bytes!("../../assets/models/player/face/eyes/4.png")),
            ("face/eyes/5.png", include_bytes!("../../assets/models/player/face/eyes/5.png")),
            ("face/eyes/6.png", include_bytes!("../../assets/models/player/face/eyes/6.png")),
            ("face/eyes/7.png", include_bytes!("../../assets/models/player/face/eyes/7.png")),
            ("face/mouths/0.png", include_bytes!("../../assets/models/player/face/mouths/0.png")),
            ("face/mouths/1.png", include_bytes!("../../assets/models/player/face/mouths/1.png")),
            ("face/mouths/2.png", include_bytes!("../../assets/models/player/face/mouths/2.png")),
            ("face/mouths/3.png", include_bytes!("../../assets/models/player/face/mouths/3.png")),
            ("face/mouths/4.png", include_bytes!("../../assets/models/player/face/mouths/4.png")),
            ("face/mouths/5.png", include_bytes!("../../assets/models/player/face/mouths/5.png")),
            ("face/masks/0.png", include_bytes!("../../assets/models/player/face/masks/0.png")),
            ("face/masks/1.png", include_bytes!("../../assets/models/player/face/masks/1.png")),
            ("face/masks/2.png", include_bytes!("../../assets/models/player/face/masks/2.png")),
            ("face/masks/3.png", include_bytes!("../../assets/models/player/face/masks/3.png")),
            ("face/masks/4.png", include_bytes!("../../assets/models/player/face/masks/4.png")),
            ("face/masks/5.png", include_bytes!("../../assets/models/player/face/masks/5.png")),
            ("face/masks/6.png", include_bytes!("../../assets/models/player/face/masks/6.png")),
            ("face/masks/7.png", include_bytes!("../../assets/models/player/face/masks/7.png")),
        ];
        for (name, bytes) in assets {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        hash.finalize().into()
    })
}
