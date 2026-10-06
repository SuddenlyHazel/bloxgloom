//! Stable botanical variation built on mesh workers, never on the window thread.
use super::{
    ChunkMesh, ResolvedBlock, ResolvedChunk, VERTEX_FLOATS, pack_light_direction, pack_light_rgb,
};
use crate::{
    content::{self, Catalog},
    lighting::LightField,
    world::{self, CHUNK_SIZE, Chunk, ChunkKey},
};
use glam::Vec3;

fn seed(cell: [f32; 3]) -> u32 {
    let [x, y, z] = cell.map(|v| v.floor() as i32 as u32);
    x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ z.wrapping_mul(0xcb1a_b31f)
}

fn unit(seed: u32, salt: u32) -> f32 {
    let mut v = seed ^ salt;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v & 65535) as f32 / 65535.0
}

/// A leaf voxel is a small intersecting cluster, rather than six cube walls.
/// Positions are local to the cell and remain inside it, including tilted tips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LeafCard {
    pub positions: [Vec3; 4],
    pub normal: Vec3,
}

const NEAR_TIMBER: u32 = 1 << 6;
const ABOVE_CAP: u32 = 1 << 7;

pub(super) fn leaf_cluster(cell: [f32; 3], exposed: u32) -> impl Iterator<Item = LeafCard> {
    let hash = seed(cell);
    let yaw = unit(hash, 71) * std::f32::consts::TAU;
    let boundary = exposed & 63;
    let body = if boundary == 0 && exposed & NEAR_TIMBER == 0 {
        0
    } else if boundary.count_ones() >= 3 {
        4
    } else {
        3
    };
    let count = body + if exposed & ABOVE_CAP != 0 { 2 } else { 0 };
    (0..count).map(move |index| {
        let salt = 101 + index * 13;
        let cap = index >= body;
        let angle = if cap {
            (index - body) as f32 * std::f32::consts::FRAC_PI_4 + (unit(hash, salt) - 0.5) * 0.12
        } else {
            yaw + index as f32 * 1.37 + (unit(hash, salt) - 0.5) * 0.5
        };
        let (sin, cos) = angle.sin_cos();
        let u = Vec3::new(cos, 0.0, sin);
        let v = if cap {
            Vec3::new(sin, 0.12 + unit(hash, salt + 1) * 0.04, -cos).normalize()
        } else if index >= 2 || (index == 1 && exposed & (1 << 3) != 0) {
            // The crown card is deliberately slanted, never a flat cube lid.
            Vec3::new(sin, 0.30 + unit(hash, salt + 1) * 0.35, -cos).normalize()
        } else {
            let tilt = (unit(hash, salt + 1) - 0.5) * 0.44;
            Vec3::new(-sin * tilt, 1.0, cos * tilt).normalize()
        };
        let normal = u.cross(v).normalize();
        let center = Vec3::new(
            0.5 + (unit(hash, salt + 2) - 0.5) * 0.16,
            if cap {
                0.10 + (index - body) as f32 * 0.09
            } else {
                0.5 + (unit(hash, salt + 3) - 0.5) * 0.16
            },
            0.5 + (unit(hash, salt + 4) - 0.5) * 0.16,
        );
        let half_u = 0.50 + unit(hash, salt + 5) * 0.08;
        let half_v = 0.46 + unit(hash, salt + 6) * 0.06;
        let extent = u.abs() * half_u + v.abs() * half_v;
        let available = center.min(Vec3::ONE - center) - Vec3::splat(0.015);
        let scale = (available / extent).min_element().min(1.0);
        let u = u * half_u * scale;
        let v = v * half_v * scale;
        LeafCard {
            positions: [
                center - u - v,
                center + u - v,
                center + u + v,
                center - u + v,
            ],
            normal,
        }
    })
}

/// Mark exposed directions using authoritative loaded neighbors at chunk seams.
/// Mixed species form one canopy. Buried foliage next to timber still exists:
/// its cutout boundary cannot hide a log face as an opaque cube wall would.
pub(super) fn leaf_exposure(
    p: [usize; 3],
    key: ChunkKey,
    resolved: &ResolvedChunk,
    catalog: &Catalog,
    known: &std::collections::HashMap<ChunkKey, std::sync::Arc<Chunk>>,
) -> u32 {
    let n = CHUNK_SIZE as i32;
    let mut exposed = 0;
    for axis in 0..3 {
        for side in [-1, 1] {
            let mut neighbor = p.map(|v| v as i32);
            neighbor[axis] += side;
            let block = if neighbor.iter().all(|v| (0..n).contains(v)) {
                Some(resolved.block_at(neighbor.map(|v| v as usize), CHUNK_SIZE))
            } else {
                let world = [key.x, key.y, key.z];
                let world = std::array::from_fn::<_, 3, _>(|i| world[i] * n + neighbor[i]);
                let (key, local) = world::world_to_chunk(world[0], world[1], world[2]);
                known
                    .get(&key)
                    .and_then(|chunk| chunk.block(local))
                    .map(|id| ResolvedBlock::new(id, catalog))
            };
            if block.is_some_and(|b| b.timber) {
                exposed |= NEAR_TIMBER;
                if axis == 1 && side == -1 {
                    exposed |= ABOVE_CAP;
                }
            }
            if block.is_none_or(|b| {
                !(b.has(content::OPAQUE)
                    || b.botanical && b.has(content::CUTOUT) && !b.has(content::PLANT))
            }) {
                exposed |= 1 << (axis * 2 + usize::from(side > 0));
            }
        }
    }
    exposed
}

pub(super) fn emit_leaf_cluster(
    out: &mut ChunkMesh,
    origin: [f32; 3],
    p: [usize; 3],
    block: ResolvedBlock,
    exposed: u32,
    light: Option<&LightField>,
) {
    let cell = std::array::from_fn(|i| origin[i] + p[i] as f32);
    for card in leaf_cluster(cell, exposed) {
        let base = (out.cutout_vertices.len() / VERTEX_FLOATS) as u32;
        let axis = if card.normal.x.abs() > card.normal.y.abs()
            && card.normal.x.abs() > card.normal.z.abs()
        {
            0
        } else if card.normal.y.abs() > card.normal.z.abs() {
            1
        } else {
            2
        };
        let layer = block.face_layer(axis, if card.normal[axis] > 0.0 { 1 } else { -1 }) as f32;
        for (position, uv) in
            card.positions
                .into_iter()
                .zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
        {
            let local = std::array::from_fn(|i| p[i] as f32 + position[i]);
            let (sample, visibility) = light
                .map_or(([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0), |field| {
                    field.spatial_with_visibility(local)
                });
            let (local_rgb, local_direction) =
                light.map_or(([0.0; 3], [0.0; 3]), |field| field.spatial_local(local));
            out.cutout_vertices.extend_from_slice(&[
                cell[0] + position.x,
                cell[1] + position.y,
                cell[2] + position.z,
                card.normal.x,
                card.normal.y,
                card.normal.z,
                uv[0],
                uv[1],
                layer + (1.0 - visibility) * 0.5,
                sample[0],
                sample[1],
                pack_light_rgb(&sample[2..5]) as f32,
                pack_light_rgb(&sample[5..8]) as f32,
                pack_light_rgb(&local_rgb) as f32,
                pack_light_direction(local_direction) as f32,
            ]);
        }
        out.cutout_indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

pub(super) fn plant_position(cell: [f32; 3], x: f32, z: f32) -> [f32; 2] {
    let hash = seed(cell);
    let angle = unit(hash, 31) * std::f32::consts::TAU;
    let scale = 0.77 + unit(hash, 47) * 0.23;
    let (sin, cos) = angle.sin_cos();
    let x = (x - 0.5) * scale;
    let z = (z - 0.5) * scale;
    [0.5 + x * cos - z * sin, 0.5 + x * sin + z * cos]
}

#[cfg(test)]
mod tests;
