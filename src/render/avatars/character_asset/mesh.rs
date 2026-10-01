//! Bounded native mesh decoding; shared JSON owns body/rig, clip files own timing.
use super::{CharacterAsset, CharacterVertex, MATERIAL_VERTEX_LIMITS};

const HAIR_MESHES: [&[u8]; 13] = [
    include_bytes!("../../../../assets/models/player/hair.mesh"),
    include_bytes!("../../../../assets/models/player/hair_undercut.mesh"),
    include_bytes!("../../../../assets/models/player/hair_space_buns.mesh"),
    include_bytes!("../../../../assets/models/player/hair_curly_bob.mesh"),
    include_bytes!("../../../../assets/models/player/hair_curly_pigtails.mesh"),
    include_bytes!("../../../../assets/models/player/hair_sidepart_bob.mesh"),
    include_bytes!("../../../../assets/models/player/hair_compact_braid.mesh"),
    include_bytes!("../../../../assets/models/player/hair_long_loose_curls.mesh"),
    include_bytes!("../../../../assets/models/player/hair_long_curly_ponytail.mesh"),
    include_bytes!("../../../../assets/models/player/hair_half_up_curly_cascade.mesh"),
    include_bytes!("../../../../assets/models/player/hair_rounded_afro.mesh"),
    include_bytes!("../../../../assets/models/player/hair_twin_braids.mesh"),
    include_bytes!("../../../../assets/models/player/hair_curly_mohawk.mesh"),
];

impl CharacterAsset {
    pub(super) fn from_builtin_parts() -> Result<Self, String> {
        let json = include_str!("../../../../assets/models/player/character.json");
        if json.len() > 256 * 1024 {
            return Err("shared character asset exceeds byte limit".into());
        }
        let mut asset: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        if asset.vertices.len() > MATERIAL_VERTEX_LIMITS[0]
            || asset.indices.len() > MATERIAL_VERTEX_LIMITS[0] * 3
            || asset.vertices.iter().any(|v| v.material != 0)
        {
            return Err("shared character JSON must contain only bounded body geometry".into());
        }
        if !asset.clips.is_empty() {
            return Err("shared character JSON must not duplicate clip data".into());
        }
        for json in [
            include_str!("../../../../assets/models/player/clip_walk.json"),
            include_str!("../../../../assets/models/player/clip_idle.json"),
            include_str!("../../../../assets/models/player/clip_crouch.json"),
            include_str!("../../../../assets/models/player/clip_tool_use_left.json"),
            include_str!("../../../../assets/models/player/clip_tool_use_right.json"),
        ] {
            if json.len() > 256 * 1024 {
                return Err("character clip exceeds byte limit".into());
            }
            asset
                .clips
                .push(serde_json::from_str(json).map_err(|error| error.to_string())?);
        }
        for (index, data) in HAIR_MESHES.iter().enumerate() {
            asset.append_hair(data, index + 1)?;
        }
        asset.validate()?;
        Ok(asset)
    }

    fn append_hair(&mut self, data: &[u8], material: usize) -> Result<(), String> {
        let Some(&limit) = MATERIAL_VERTEX_LIMITS
            .get(material)
            .filter(|_| material > 0)
        else {
            return Err("unknown native hair material".into());
        };
        if data.len() < 12 || &data[..4] != b"BGH1" {
            return Err("invalid native hair mesh header".into());
        }
        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let indices = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        if count == 0
            || count > limit
            || indices == 0
            || indices > limit * 3
            || !indices.is_multiple_of(3)
            || data.len() != 12 + count * 32 + indices * 2
        {
            return Err("invalid native hair mesh bounds".into());
        }
        let first = self.vertices.len() as u32;
        for vertex in data[12..12 + count * 32].chunks_exact(32) {
            let values: [f32; 8] = std::array::from_fn(|i| {
                f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
            });
            self.vertices.push(CharacterVertex {
                position: values[..3].try_into().unwrap(),
                normal: values[3..6].try_into().unwrap(),
                uv: values[6..8].try_into().unwrap(),
                joint: 1,
                material: material as u32,
            });
        }
        for bytes in data[12 + count * 32..].chunks_exact(2) {
            let index = u16::from_le_bytes(bytes.try_into().unwrap());
            if usize::from(index) >= count {
                return Err("native hair triangle index out of bounds".into());
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
    fn malformed_native_hair_fails_before_rendering() {
        for mutate in 0..5 {
            let mut data = HAIR_MESHES[0].to_vec();
            match mutate {
                0 => data[0] = 0,
                1 => {
                    data.pop();
                }
                2 => data[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
                3 => data[8..12].copy_from_slice(&u32::MAX.to_le_bytes()),
                _ => {
                    let len = data.len();
                    data[len - 2..].copy_from_slice(&u16::MAX.to_le_bytes());
                }
            }
            let mut asset = CharacterAsset::builtin();
            assert!(asset.append_hair(&data, 1).is_err());
        }
        assert!(
            CharacterAsset::builtin()
                .append_hair(HAIR_MESHES[0], 14)
                .is_err()
        );
    }
}
