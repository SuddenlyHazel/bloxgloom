use super::*;

#[test]
fn builtin_visibility_matches_meshes_is_subtle_and_preserves_face_features() {
    let asset = super::super::CharacterAsset::builtin();
    let visibility = builtin_visibility(asset.vertices.len());
    for (vertex, &value) in asset.vertices.iter().zip(&visibility) {
        assert!((MIN_VISIBILITY..=255).contains(&value));
        if (2..=6).contains(&vertex.surface) {
            assert_eq!(value, 255, "source-authored face features stay untouched");
        }
        let packed = pack_surface(vertex.surface, value);
        assert_eq!(packed & 255, vertex.surface);
        assert_eq!((packed >> 8) & 255, u32::from(value));
        assert_eq!(packed >> 16, 0);
    }
    // Protect against accidentally shipping an empty/all-open bake. Each style
    // has local clumps, while most of its hemisphere remains visible.
    for material in 1..14 {
        let values: Vec<_> = asset
            .vertices
            .iter()
            .zip(&visibility)
            .filter_map(|(vertex, &value)| (vertex.material == material).then_some(value))
            .collect();
        assert!(values.iter().any(|&v| v < 250), "hair {material}");
        assert!(values.contains(&255), "hair {material}");
        let mean = values.iter().map(|&v| f32::from(v)).sum::<f32>() / values.len() as f32;
        assert!(mean > 220.0, "hair {material}: mean visibility {mean}");
    }
}

#[test]
fn stale_truncated_excessive_or_out_of_bounds_bakes_are_rejected() {
    for mutation in 0..8 {
        let mut data = BAKED.to_vec();
        match mutation {
            0 => data[0] = 0,
            1 => data[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
            2 => data[8] ^= 1, // same length with a different native mesh hash
            3 => data[40..44].copy_from_slice(&u32::MAX.to_le_bytes()),
            4 => data[44] = MIN_VISIBILITY - 1,
            5 => {
                data.pop();
            }
            6 => data.push(255),
            _ => data.truncate(7),
        }
        assert!(decode(&data, &super::super::mesh::MESHES).is_err());
    }
}
