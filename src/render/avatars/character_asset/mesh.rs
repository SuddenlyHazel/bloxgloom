//! Bounded native rigid meshes; conversion and asset validation happen offline.
use super::{CharacterAsset, CharacterVertex};
const MESHES: [&[u8]; 15] = [
    include_bytes!("../../../../assets/models/player/articulated/flat_chest.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/tousled_crop.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/side_swept_undercut.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/space_buns.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/curly_bob.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/curly_pigtails.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/sidepart_bob.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/compact_braid.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/long_loose_curls.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/long_curly_ponytail.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/half_up_curly_cascade.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/rounded_afro.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/twin_braids.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/curly_mohawk.mesh"),
    include_bytes!("../../../../assets/models/player/articulated/defined_chest_sports_bra.mesh"),
];
impl CharacterAsset {
    pub(super) fn from_builtin_parts() -> Result<Self, String> {
        let mut asset: Self = serde_json::from_str(include_str!(
            "../../../../assets/models/player/articulated/rig.json"
        ))
        .map_err(|e| e.to_string())?;
        if !asset.vertices.is_empty() || !asset.indices.is_empty() {
            return Err("rig must not duplicate meshes".into());
        }
        for source in [
            include_str!("../../../../assets/models/player/articulated/clip_crouch_test.json"),
            include_str!("../../../../assets/models/player/articulated/clip_grip_test.json"),
            include_str!("../../../../assets/models/player/articulated/clip_weight_shift.json"),
            include_str!("../../../../assets/models/player/articulated/clip_wrist_ankle_test.json"),
        ] {
            if source.len() > 256 * 1024 {
                return Err("source clip byte limit exceeded".into());
            }
            asset
                .clips
                .push(serde_json::from_str(source).map_err(|e| e.to_string())?);
        }
        for (material, data) in MESHES.iter().enumerate() {
            asset.append_mesh(data, material)?;
        }
        asset.validate()?;
        Ok(asset)
    }
    fn append_mesh(&mut self, data: &[u8], material: usize) -> Result<(), String> {
        if material >= MESHES.len() || data.len() < 12 || &data[..4] != b"BGC2" {
            return Err("invalid native mesh header".into());
        }
        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let indices = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        if count == 0
            || count > 16384
            || indices == 0
            || indices > 49152
            || !indices.is_multiple_of(3)
            || data.len() != 12 + count * 44 + indices * 2
        {
            return Err("invalid native mesh bounds".into());
        }
        let first = self.vertices.len() as u32;
        for vertex in data[12..12 + count * 44].chunks_exact(44) {
            let values: [f32; 8] = std::array::from_fn(|i| {
                f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let joint = u32::from_le_bytes(vertex[32..36].try_into().unwrap());
            let actual_material = u32::from_le_bytes(vertex[36..40].try_into().unwrap());
            let surface = u32::from_le_bytes(vertex[40..44].try_into().unwrap());
            if actual_material as usize != material {
                return Err("native mesh material mismatch".into());
            }
            self.vertices.push(CharacterVertex {
                position: values[..3].try_into().unwrap(),
                normal: values[3..6].try_into().unwrap(),
                uv: values[6..].try_into().unwrap(),
                joint: joint as usize,
                material: actual_material,
                surface,
            });
        }
        for bytes in data[12 + count * 44..].chunks_exact(2) {
            let index = u16::from_le_bytes(bytes.try_into().unwrap());
            if usize::from(index) >= count {
                return Err("native mesh index out of bounds".into());
            }
            self.indices.push(first + u32::from(index));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_native_mesh_fails_before_rendering() {
        for mutation in 0..6 {
            let mut data = MESHES[0].to_vec();
            match mutation {
                0 => data[0] = 0,
                1 => {
                    data.pop();
                }
                2 => data[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
                3 => data[8..12].copy_from_slice(&u32::MAX.to_le_bytes()),
                4 => data[48..52].copy_from_slice(&99u32.to_le_bytes()),
                _ => {
                    let n = data.len();
                    data[n - 2..].copy_from_slice(&u16::MAX.to_le_bytes());
                }
            }
            assert!(CharacterAsset::builtin().append_mesh(&data, 0).is_err());
        }
        assert!(
            CharacterAsset::builtin()
                .append_mesh(MESHES[0], 15)
                .is_err()
        );
    }
}
