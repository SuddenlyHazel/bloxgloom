//! Server-authored trees keep their metre-wide stems and cutout crowns at distance.
//! No world generation runs here: dimensions, species and materials come from the tile.
use super::{FaceColors, mesh::Mesh, surface::Surface, vertex::Vertex};
use crate::{
    content::{BlockStateId, Catalog},
    lod::TreeFeature,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
mod support;
pub(super) use support::root_base;

pub(super) fn append(
    mesh: &mut Mesh,
    tree: &TreeFeature,
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    append_geometry(mesh, tree, tree.anchor[1] + 1, catalog, colors)
}

pub(super) fn append_supported(
    mesh: &mut Mesh,
    tree: &TreeFeature,
    base: i32,
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    append_geometry(mesh, tree, base, catalog, colors)
}

fn append_geometry(
    mesh: &mut Mesh,
    tree: &TreeFeature,
    base: i32,
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    let vertex_start = mesh.vertices.len();
    let index_start = mesh.indices.len();
    let [ox, oz, mx, mz] = mesh.key.bounds().ok_or("invalid forest tile bounds")?;
    let anchor = [
        i64::from(tree.anchor[0]) - i64::from(ox),
        i64::from(tree.anchor[1]),
        i64::from(tree.anchor[2]) - i64::from(oz),
    ];
    let top = anchor[1] + i64::from(tree.trunk_height);
    let limit = [
        (i64::from(mx) - i64::from(ox)) as f32,
        (i64::from(mz) - i64::from(oz)) as f32,
    ];
    let mut logs = BTreeMap::new();
    for y in i64::from(base)..=top {
        logs.insert([anchor[0], y, anchor[2]], tree.log);
    }
    if tree.species != 1 {
        for branch in 0..3 {
            let (dx, dz) = direction((tree.shape + branch) & 3);
            let reach = if matches!(tree.species, 4 | 7) { 3 } else { 2 };
            let by = branch_height(tree, branch);
            for step in 1..=reach {
                for dy in [-1, 0] {
                    logs.insert(
                        [
                            anchor[0] + dx * step,
                            top + by - reach + step + dy,
                            anchor[2] + dz * step,
                        ],
                        if dx != 0 {
                            tree.branch_x
                        } else {
                            tree.branch_z
                        },
                    );
                }
            }
        }
    }
    append_logs(mesh, &logs, limit, catalog, colors)?;
    let center = [
        anchor[0] as f32 + 0.5,
        top as f32 + 0.5,
        anchor[2] as f32 + 0.5,
    ];
    if tree.species == 1 {
        // The actual spruce has an exposed stem, three narrowing tiers and a leader.
        for (bottom, height, radius) in [(-8.5, 4.0, 3.5), (-4.5, 3.5, 2.5), (-1.5, 4.0, 1.5)] {
            for axis in [0, 2] {
                let outline = [
                    (-radius, bottom),
                    (-radius * 0.65, bottom + height * 0.55),
                    (0.0, bottom + height),
                    (radius * 0.65, bottom + height * 0.55),
                    (radius, bottom),
                ];
                let mut polygon: Vec<_> = outline
                    .into_iter()
                    .map(|(horizontal, vertical)| {
                        let mut p = center;
                        p[1] += vertical;
                        p[if axis == 0 { 2 } else { 0 }] += horizontal;
                        p
                    })
                    .collect();
                // Maintain cardinal positive winding, regardless of card orientation.
                if axis == 2 {
                    polygon.reverse();
                }
                leaf_card(mesh, &polygon, axis, tree.leaves, limit, catalog, colors)?;
            }
            ellipse(
                mesh,
                [center[0], center[1] + bottom + 0.45, center[2]],
                [radius, 0.0, radius],
                1,
                4,
                tree.leaves,
                limit,
                catalog,
                colors,
            )?;
        }
    } else {
        crown(mesh, center, 2.9, 2.1, tree.leaves, limit, catalog, colors)?;
        for branch in 0..3 {
            let (dx, dz) = direction((tree.shape + branch) & 3);
            let reach = if matches!(tree.species, 4 | 7) {
                3.0
            } else {
                2.0
            };
            let c = [
                center[0] + dx as f32 * reach,
                center[1] + branch_height(tree, branch) as f32,
                center[2] + dz as f32 * reach,
            ];
            crown(
                mesh,
                c,
                2.9,
                if tree.species == 4 { 1.5 } else { 2.1 },
                tree.leaves,
                limit,
                catalog,
                colors,
            )?;
        }
    }
    compact_feature(mesh, vertex_start, index_start);
    if mesh.byte_len() > 8 * 1024 * 1024 {
        return Err("LOD forest exceeds 8 MiB geometry budget".into());
    }
    Ok(())
}

/// Share matching rectangle corners and crown contours within one feature.
/// The complete packed vertex key preserves normal, material, color and light seams.
fn compact_feature(mesh: &mut Mesh, vertex_start: usize, index_start: usize) {
    let mut unique = HashMap::<[u32; 5], u32>::new();
    let mut remap = Vec::with_capacity(mesh.vertices.len() - vertex_start);
    let mut vertices = Vec::with_capacity(mesh.vertices.len() - vertex_start);
    for &vertex in &mesh.vertices[vertex_start..] {
        let key = bytemuck::cast(vertex);
        let next = (vertex_start + vertices.len()) as u32;
        let index = *unique.entry(key).or_insert_with(|| {
            vertices.push(vertex);
            next
        });
        remap.push(index);
    }
    for index in &mut mesh.indices[index_start..] {
        *index = remap[*index as usize - vertex_start];
    }
    mesh.vertices.truncate(vertex_start);
    mesh.vertices.extend(vertices);
}

fn direction(angle: u8) -> (i64, i64) {
    match angle {
        0 => (1, 0),
        1 => (0, 1),
        2 => (-1, 0),
        _ => (0, -1),
    }
}
fn branch_height(tree: &TreeFeature, branch: u8) -> i64 {
    if tree.species == 4 {
        -1
    } else {
        -2 + i64::from((tree.shape >> (2 + branch)) & 1)
    }
}

type Cells = BTreeMap<[i64; 3], BlockStateId>;
fn append_logs(
    mesh: &mut Mesh,
    cells: &Cells,
    limit: [f32; 2],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    let mut faces = BTreeMap::<(usize, i64, i32, BlockStateId), BTreeSet<(i64, i64)>>::new();
    for (&p, &state) in cells {
        for axis in 0..3 {
            for side in [-1, 1] {
                let mut neighbor = p;
                neighbor[axis] += i64::from(side);
                if cells.contains_key(&neighbor) {
                    continue;
                }
                faces
                    .entry((axis, p[axis] + i64::from(side > 0), side, state))
                    .or_default()
                    .insert((p[(axis + 1) % 3], p[(axis + 2) % 3]));
            }
        }
    }
    for ((axis, plane, side, state), mut cells) in faces {
        while let Some(&(u, v)) = cells.first() {
            let mut width = 1;
            while cells.contains(&(u + width, v)) {
                width += 1;
            }
            let mut height = 1;
            while (u..u + width).all(|a| cells.contains(&(a, v + height))) {
                height += 1;
            }
            for a in u..u + width {
                for b in v..v + height {
                    cells.remove(&(a, b));
                }
            }
            let polygon: Vec<_> = [
                (u, v),
                (u + width, v),
                (u + width, v + height),
                (u, v + height),
            ]
            .into_iter()
            .map(|(a, b)| {
                let mut p = [0.0; 3];
                p[axis] = plane as f32;
                p[(axis + 1) % 3] = a as f32;
                p[(axis + 2) % 3] = b as f32;
                p
            })
            .collect();
            emit(mesh, &polygon, axis, side, state, limit, catalog, colors)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn crown(
    mesh: &mut Mesh,
    center: [f32; 3],
    radius: f32,
    height: f32,
    leaves: BlockStateId,
    limit: [f32; 2],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    for axis in [0, 2] {
        ellipse(
            mesh,
            center,
            [radius, height, radius],
            axis,
            8,
            leaves,
            limit,
            catalog,
            colors,
        )?;
    }
    // A small horizontal card retains a canopy from high viewpoints, without a solid roof.
    ellipse(
        mesh,
        center,
        [radius, height, radius],
        1,
        4,
        leaves,
        limit,
        catalog,
        colors,
    )
}
#[allow(clippy::too_many_arguments)]
fn ellipse(
    mesh: &mut Mesh,
    center: [f32; 3],
    radius: [f32; 3],
    axis: usize,
    count: usize,
    state: BlockStateId,
    limit: [f32; 2],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    let polygon: Vec<_> = (0..count)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / count as f32;
            let mut p = center;
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            p[u] += radius[u] * angle.cos();
            p[v] += radius[v] * angle.sin();
            p
        })
        .collect();
    leaf_card(mesh, &polygon, axis, state, limit, catalog, colors)
}
#[allow(clippy::too_many_arguments)]
fn leaf_card(
    mesh: &mut Mesh,
    polygon: &[[f32; 3]],
    axis: usize,
    state: BlockStateId,
    limit: [f32; 2],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    for side in [-1, 1] {
        emit(mesh, polygon, axis, side, state, limit, catalog, colors)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn emit(
    mesh: &mut Mesh,
    polygon: &[[f32; 3]],
    axis: usize,
    side: i32,
    state: BlockStateId,
    limit: [f32; 2],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<(), String> {
    let polygon = clip(polygon, limit);
    if polygon.len() < 3 {
        return Ok(());
    }
    let mut surface = Surface::new(catalog, colors, state, axis, side, 0);
    // Foliage needs real alpha holes even at coarse levels. Bark preserves its art too.
    surface.sample_texture = true;
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend(polygon.iter().map(|&p| {
        Vertex::new(
            p,
            axis,
            side,
            [surface.color[0], surface.color[1], surface.color[2]],
            15,
            0,
        )
        .material(surface)
    }));
    for i in 1..polygon.len() as u32 - 1 {
        mesh.indices.extend(if side > 0 {
            [base, base + i, base + i + 1]
        } else {
            [base, base + i + 1, base + i]
        });
    }
    Ok(())
}

/// Clip existing surfaces; never introduce tile-edge caps or change crown dimensions by LOD.
fn clip(input: &[[f32; 3]], limit: [f32; 2]) -> Vec<[f32; 3]> {
    let mut polygon = input.to_vec();
    for (axis, maximum) in [(0, limit[0]), (2, limit[1])] {
        // Half-open ownership also avoids duplicate coplanar bark at a tile edge.
        if polygon.iter().all(|p| p[axis] == maximum) {
            return Vec::new();
        }
        for (boundary, sign) in [(0.0, 1.0), (maximum, -1.0)] {
            let mut result = Vec::new();
            let Some(&last) = polygon.last() else {
                return result;
            };
            let mut previous = last;
            let mut previous_inside = (previous[axis] - boundary) * sign >= 0.0;
            for current in polygon {
                let inside = (current[axis] - boundary) * sign >= 0.0;
                if inside != previous_inside {
                    let t = (boundary - previous[axis]) / (current[axis] - previous[axis]);
                    let mut point =
                        std::array::from_fn(|a| previous[a] + t * (current[a] - previous[a]));
                    point[axis] = boundary;
                    result.push(point);
                }
                if inside {
                    result.push(current);
                }
                previous = current;
                previous_inside = inside;
            }
            result.dedup();
            if result.len() > 1 && result.first() == result.last() {
                result.pop();
            }
            polygon = result;
        }
    }
    let mut area = [0.0; 3];
    if let Some(&first) = polygon.first() {
        for pair in polygon[1..].windows(2) {
            let a: [f32; 3] = std::array::from_fn(|i| pair[0][i] - first[i]);
            let b: [f32; 3] = std::array::from_fn(|i| pair[1][i] - first[i]);
            area[0] += a[1] * b[2] - a[2] * b[1];
            area[1] += a[2] * b[0] - a[0] * b[2];
            area[2] += a[0] * b[1] - a[1] * b[0];
        }
    }
    if area.iter().all(|a: &f32| a.abs() < 1e-6) {
        polygon.clear();
    }
    polygon
}

#[cfg(test)]
mod tests;
