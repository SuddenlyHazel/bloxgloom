//! Exact builtin catalog identity, cached once for repeated catalog comparisons.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(super) fn fingerprint() -> &'static [u8; 32] {
    static HASH: OnceLock<[u8; 32]> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        hash.update(b"bloxgloom-character-recipe/v2;player-payload/v3;iris-option/v1;hair-srgb/v1;basis=y-up,+z,1.0;articulated-only/v1");
        hash.update(super::CharacterRecipe::default().encode());
        for (group, names) in [("bodies", super::BODIES.as_slice()), ("eyes", super::EYES.as_slice()), ("mouths", super::MOUTHS.as_slice()), ("hair", super::HAIR.as_slice())] {
            hash.update(group.as_bytes());
            hash.update((names.len() as u64).to_le_bytes());
            for name in names {
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
            }
        }
        let assets: &[(&str, &[u8])] = &[
            ("articulated/clip_crouch_test.json", include_bytes!("../../assets/models/player/articulated/clip_crouch_test.json")),
            ("articulated/clip_grip_test.json", include_bytes!("../../assets/models/player/articulated/clip_grip_test.json")),
            ("articulated/clip_weight_shift.json", include_bytes!("../../assets/models/player/articulated/clip_weight_shift.json")),
            ("articulated/clip_wrist_ankle_test.json", include_bytes!("../../assets/models/player/articulated/clip_wrist_ankle_test.json")),
            ("articulated/compact_braid.mesh", include_bytes!("../../assets/models/player/articulated/compact_braid.mesh")),
            ("articulated/compact_braid_accessory.png", include_bytes!("../../assets/models/player/articulated/compact_braid_accessory.png")),
            ("articulated/compact_braid_neutral.png", include_bytes!("../../assets/models/player/articulated/compact_braid_neutral.png")),
            ("articulated/curly_bob.mesh", include_bytes!("../../assets/models/player/articulated/curly_bob.mesh")),
            ("articulated/curly_bob_accessory.png", include_bytes!("../../assets/models/player/articulated/curly_bob_accessory.png")),
            ("articulated/curly_bob_neutral.png", include_bytes!("../../assets/models/player/articulated/curly_bob_neutral.png")),
            ("articulated/curly_mohawk.mesh", include_bytes!("../../assets/models/player/articulated/curly_mohawk.mesh")),
            ("articulated/curly_mohawk_accessory.png", include_bytes!("../../assets/models/player/articulated/curly_mohawk_accessory.png")),
            ("articulated/curly_mohawk_neutral.png", include_bytes!("../../assets/models/player/articulated/curly_mohawk_neutral.png")),
            ("articulated/curly_pigtails.mesh", include_bytes!("../../assets/models/player/articulated/curly_pigtails.mesh")),
            ("articulated/curly_pigtails_accessory.png", include_bytes!("../../assets/models/player/articulated/curly_pigtails_accessory.png")),
            ("articulated/curly_pigtails_neutral.png", include_bytes!("../../assets/models/player/articulated/curly_pigtails_neutral.png")),
            ("articulated/defined_chest_sports_bra.mesh", include_bytes!("../../assets/models/player/articulated/defined_chest_sports_bra.mesh")),
            ("articulated/defined_chest_sports_bra.png", include_bytes!("../../assets/models/player/articulated/defined_chest_sports_bra.png")),
            ("articulated/flat_chest.mesh", include_bytes!("../../assets/models/player/articulated/flat_chest.mesh")),
            ("articulated/flat_chest.png", include_bytes!("../../assets/models/player/articulated/flat_chest.png")),
            ("articulated/half_up_curly_cascade.mesh", include_bytes!("../../assets/models/player/articulated/half_up_curly_cascade.mesh")),
            ("articulated/half_up_curly_cascade_accessory.png", include_bytes!("../../assets/models/player/articulated/half_up_curly_cascade_accessory.png")),
            ("articulated/half_up_curly_cascade_neutral.png", include_bytes!("../../assets/models/player/articulated/half_up_curly_cascade_neutral.png")),
            ("articulated/long_curly_ponytail.mesh", include_bytes!("../../assets/models/player/articulated/long_curly_ponytail.mesh")),
            ("articulated/long_curly_ponytail_accessory.png", include_bytes!("../../assets/models/player/articulated/long_curly_ponytail_accessory.png")),
            ("articulated/long_curly_ponytail_neutral.png", include_bytes!("../../assets/models/player/articulated/long_curly_ponytail_neutral.png")),
            ("articulated/long_loose_curls.mesh", include_bytes!("../../assets/models/player/articulated/long_loose_curls.mesh")),
            ("articulated/long_loose_curls_accessory.png", include_bytes!("../../assets/models/player/articulated/long_loose_curls_accessory.png")),
            ("articulated/long_loose_curls_neutral.png", include_bytes!("../../assets/models/player/articulated/long_loose_curls_neutral.png")),
            ("articulated/rig.json", include_bytes!("../../assets/models/player/articulated/rig.json")),
            ("articulated/rounded_afro.mesh", include_bytes!("../../assets/models/player/articulated/rounded_afro.mesh")),
            ("articulated/rounded_afro_accessory.png", include_bytes!("../../assets/models/player/articulated/rounded_afro_accessory.png")),
            ("articulated/rounded_afro_neutral.png", include_bytes!("../../assets/models/player/articulated/rounded_afro_neutral.png")),
            ("articulated/side_swept_undercut.mesh", include_bytes!("../../assets/models/player/articulated/side_swept_undercut.mesh")),
            ("articulated/side_swept_undercut_accessory.png", include_bytes!("../../assets/models/player/articulated/side_swept_undercut_accessory.png")),
            ("articulated/side_swept_undercut_neutral.png", include_bytes!("../../assets/models/player/articulated/side_swept_undercut_neutral.png")),
            ("articulated/sidepart_bob.mesh", include_bytes!("../../assets/models/player/articulated/sidepart_bob.mesh")),
            ("articulated/sidepart_bob_accessory.png", include_bytes!("../../assets/models/player/articulated/sidepart_bob_accessory.png")),
            ("articulated/sidepart_bob_neutral.png", include_bytes!("../../assets/models/player/articulated/sidepart_bob_neutral.png")),
            ("articulated/space_buns.mesh", include_bytes!("../../assets/models/player/articulated/space_buns.mesh")),
            ("articulated/space_buns_accessory.png", include_bytes!("../../assets/models/player/articulated/space_buns_accessory.png")),
            ("articulated/space_buns_neutral.png", include_bytes!("../../assets/models/player/articulated/space_buns_neutral.png")),
            ("articulated/tousled_crop.mesh", include_bytes!("../../assets/models/player/articulated/tousled_crop.mesh")),
            ("articulated/tousled_crop_accessory.png", include_bytes!("../../assets/models/player/articulated/tousled_crop_accessory.png")),
            ("articulated/tousled_crop_neutral.png", include_bytes!("../../assets/models/player/articulated/tousled_crop_neutral.png")),
            ("articulated/twin_braids.mesh", include_bytes!("../../assets/models/player/articulated/twin_braids.mesh")),
            ("articulated/twin_braids_accessory.png", include_bytes!("../../assets/models/player/articulated/twin_braids_accessory.png")),
            ("articulated/twin_braids_neutral.png", include_bytes!("../../assets/models/player/articulated/twin_braids_neutral.png")),
            ("face/clean.png", include_bytes!("../../assets/models/player/face/clean.png")),
            ("face/eyes/0.png", include_bytes!("../../assets/models/player/face/eyes/0.png")),
            ("face/eyes/1.png", include_bytes!("../../assets/models/player/face/eyes/1.png")),
            ("face/eyes/2.png", include_bytes!("../../assets/models/player/face/eyes/2.png")),
            ("face/eyes/3.png", include_bytes!("../../assets/models/player/face/eyes/3.png")),
            ("face/eyes/4.png", include_bytes!("../../assets/models/player/face/eyes/4.png")),
            ("face/eyes/5.png", include_bytes!("../../assets/models/player/face/eyes/5.png")),
            ("face/eyes/6.png", include_bytes!("../../assets/models/player/face/eyes/6.png")),
            ("face/eyes/7.png", include_bytes!("../../assets/models/player/face/eyes/7.png")),
            ("face/masks/0.png", include_bytes!("../../assets/models/player/face/masks/0.png")),
            ("face/masks/1.png", include_bytes!("../../assets/models/player/face/masks/1.png")),
            ("face/masks/2.png", include_bytes!("../../assets/models/player/face/masks/2.png")),
            ("face/masks/3.png", include_bytes!("../../assets/models/player/face/masks/3.png")),
            ("face/masks/4.png", include_bytes!("../../assets/models/player/face/masks/4.png")),
            ("face/masks/5.png", include_bytes!("../../assets/models/player/face/masks/5.png")),
            ("face/masks/6.png", include_bytes!("../../assets/models/player/face/masks/6.png")),
            ("face/masks/7.png", include_bytes!("../../assets/models/player/face/masks/7.png")),
            ("face/mouths/0.png", include_bytes!("../../assets/models/player/face/mouths/0.png")),
            ("face/mouths/1.png", include_bytes!("../../assets/models/player/face/mouths/1.png")),
            ("face/mouths/2.png", include_bytes!("../../assets/models/player/face/mouths/2.png")),
            ("face/mouths/3.png", include_bytes!("../../assets/models/player/face/mouths/3.png")),
            ("face/mouths/4.png", include_bytes!("../../assets/models/player/face/mouths/4.png")),
            ("face/mouths/5.png", include_bytes!("../../assets/models/player/face/mouths/5.png")),
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
