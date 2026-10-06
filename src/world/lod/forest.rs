//! Deterministic server-only tree descriptors with a bounded outer-ring density.
use super::LodSampler;
use crate::lod::{TileKey, TreeFeature};
use std::collections::BTreeMap;

pub(super) fn features(sampler: &mut LodSampler, key: TileKey) -> Result<Vec<TreeFeature>, String> {
    let [x, z, mx, mz] = key.bounds().ok_or("invalid forest footprint")?;
    let width = key.sample_width().ok_or("invalid forest level")?;
    let intersects = |feature: &TreeFeature| {
        feature
            .bounds()
            .is_some_and(|(min, max)| min[0] < mx && max[0] > x && min[2] < mz && max[2] > z)
    };
    let mut out = BTreeMap::new();
    if width < 16 {
        for cz in (i64::from(z) - 6).div_euclid(12)..=(i64::from(mz) + 6).div_euclid(12) {
            for cx in (i64::from(x) - 6).div_euclid(12)..=(i64::from(mx) + 6).div_euclid(12) {
                if let Some(feature) = sampler.tree_feature(cx, cz)
                    && intersects(&feature)
                {
                    out.insert(feature.anchor, feature);
                }
            }
        }
    } else {
        // Select one actual source tree per global sample cell. Selection never depends
        // on a tile's payload pressure, so adjacent tiles choose the same seam trees.
        // Only the distant outer ring reduces density; positions/shapes are not enlarged.
        let spacing = i64::from(width.max(24));
        for bz in (i64::from(z) - 6).div_euclid(spacing)..=(i64::from(mz) + 6).div_euclid(spacing) {
            for bx in
                (i64::from(x) - 6).div_euclid(spacing)..=(i64::from(mx) + 6).div_euclid(spacing)
            {
                let minimum = [bx * spacing, bz * spacing];
                let maximum = [minimum[0] + spacing, minimum[1] + spacing];
                let mut candidates = Vec::new();
                for cz in minimum[1].div_euclid(12)..=maximum[1].div_euclid(12) {
                    for cx in minimum[0].div_euclid(12)..=maximum[0].div_euclid(12) {
                        if let Some(feature) = sampler.tree_feature(cx, cz) {
                            let fx = i64::from(feature.anchor[0]);
                            let fz = i64::from(feature.anchor[2]);
                            if fx >= minimum[0]
                                && fx < maximum[0]
                                && fz >= minimum[1]
                                && fz < maximum[1]
                            {
                                let dx = fx * 2 - minimum[0] * 2 - spacing;
                                let dz = fz * 2 - minimum[1] * 2 - spacing;
                                candidates.push((dx * dx + dz * dz, feature.anchor, feature));
                            }
                        }
                    }
                }
                if let Some((_, _, feature)) = candidates
                    .into_iter()
                    .min_by_key(|(distance, anchor, _)| (*distance, *anchor))
                    && intersects(&feature)
                {
                    out.insert(feature.anchor, feature);
                }
            }
        }
    }
    Ok(out.into_values().collect())
}

/// Support certification uses the same global ground sample as the tile.
/// Anchors just outside the footprint need this server-owned decision too so
/// both clipped crown halves either draw or omit together.
pub(super) fn assign_support(
    sampler: &mut LodSampler,
    key: TileKey,
    catalog: &crate::content::Catalog,
    columns: &[crate::lod::Column],
    trees: &mut [TreeFeature],
) -> Result<(), String> {
    let [ox, oz, _, _] = key.bounds().ok_or("invalid forest support tile")?;
    let width = key.sample_width().ok_or("invalid forest support level")?;
    let mut outside = BTreeMap::new();
    for tree in trees {
        let x = (i64::from(tree.anchor[0]) - i64::from(ox)).div_euclid(i64::from(width));
        let z = (i64::from(tree.anchor[2]) - i64::from(oz)).div_euclid(i64::from(width));
        if (0..32).contains(&x) && (0..32).contains(&z) {
            tree.support_y = tree.support_in_column(&columns[(x + 32 * z) as usize], catalog);
        } else {
            let wx = tree.anchor[0].div_euclid(width) * width;
            let wz = tree.anchor[2].div_euclid(width) * width;
            if let std::collections::btree_map::Entry::Vacant(entry) = outside.entry((wx, wz)) {
                let bottom = crate::world::BEDROCK_Y;
                let top = (crate::world::MAX_GENERATED_HEIGHT.div_euclid(16) + 1) * 16;
                let states = super::sampling::cell_for_forest(
                    sampler, wx, wz, width, bottom, top, catalog, true,
                )?;
                entry.insert(super::sampling::column(
                    states,
                    vec![crate::lod::Interval { bottom, top }],
                    catalog,
                )?);
            }
            tree.support_y = tree.support_in_column(&outside[&(wx, wz)], catalog);
        }
    }
    Ok(())
}

/// Any actual edited voxel makes this tile use the existing snapshot/span path.
/// Cached unchanged chunks do not suppress trees. This conservative tile switch
/// never lets a generated proxy overwrite a saved structure or a removed tree.
pub(super) fn has_edits(
    sampler: &mut LodSampler,
    key: TileKey,
    overlays: &[crate::world::Chunk],
) -> Result<bool, String> {
    let [minx, minz, maxx, maxz] = key.bounds().ok_or("invalid edited forest footprint")?;
    for chunk in overlays {
        if chunk.blocks.len() != crate::world::CHUNK_SIZE.pow(3) {
            return Err("invalid forest overlay size".into());
        }
        let coordinate = |value: i32| {
            value
                .checked_mul(crate::world::CHUNK_SIZE as i32)
                .ok_or("forest overlay coordinate overflow")
        };
        let [cx, cy, cz] = [
            coordinate(chunk.key.x)?,
            coordinate(chunk.key.y)?,
            coordinate(chunk.key.z)?,
        ];
        let top = cy
            .checked_add(crate::world::CHUNK_SIZE as i32)
            .ok_or("forest overlay height overflow")?;
        for z in 0..crate::world::CHUNK_SIZE {
            for x in 0..crate::world::CHUNK_SIZE {
                let wx = cx
                    .checked_add(x as i32)
                    .ok_or("forest overlay x overflow")?;
                let wz = cz
                    .checked_add(z as i32)
                    .ok_or("forest overlay z overflow")?;
                if wx < minx || wx >= maxx || wz < minz || wz >= maxz {
                    continue;
                }
                let baseline = sampler.column(i64::from(wx), i64::from(wz), cy, top);
                for (y, original) in baseline.into_iter().enumerate() {
                    if chunk
                        .block([x, y, z])
                        .ok_or("missing forest overlay voxel")?
                        != original
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;
