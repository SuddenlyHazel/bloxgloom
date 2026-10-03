//! Small, floor-validated player contact shadows. These are presentation-only
//! darkening decals, not a light source or a substitute for voxel occlusion.
//!
//! Only resident, exposed, opaque cube tops at the center support height can
//! receive a patch. Missing cells and other geometry fail closed. The footprint
//! is clipped to a connected set of those tops, never projected down a ledge.
//! Emissive receivers are excluded. Callers disable this optional pass when
//! custom material shaders are installed: arbitrary deformation/emission cannot
//! be represented safely by the floor validation and alpha-darkening blend.
mod gpu;
pub(crate) use gpu::Renderer;

use super::{AvatarModel, MAX_AVATARS, VisualAvatar};
use crate::content::{Catalog, OPAQUE, SOLID};
use crate::world::{AIR, BlockId};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

const MAX_CHARACTERS: usize = 128;
const MAX_HEIGHT: f32 = 1.25;
const MAX_DISTANCE: f32 = 28.0;
const MAX_CELLS: usize = 9;
const MAX_PATCHES: usize = MAX_CHARACTERS * MAX_CELLS;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Patch {
    /// Clipped world-space x/z rectangle.
    bounds: [f32; 4],
    /// Footprint x/z center, receiving top height and soft radius.
    center: [f32; 4],
    /// Opacity, normalized sky/glow samples, reserved.
    light: [f32; 4],
}

/// Bounded constant-size block queries, with no generation or relighting. Call
/// with the same presented positions as the avatar renderer, after animation.
/// The caller supplies authoritative resident cells; `None` is never air.
pub(crate) fn patches(
    avatars: &[VisualAvatar],
    eye: Vec3,
    catalog: &Catalog,
    mut block: impl FnMut(i32, i32, i32) -> Option<BlockId>,
) -> Vec<Patch> {
    let mut out = Vec::new();
    let mut characters = 0;
    for avatar in avatars.iter().take(MAX_AVATARS) {
        if !matches!(
            avatar.model,
            AvatarModel::Player | AvatarModel::PackagedPlayer(_)
        ) || !avatar.position.is_finite()
            || avatar.position.abs().max_element() > 1_000_000.0
            || avatar.position.distance_squared(eye) > MAX_DISTANCE * MAX_DISTANCE
            || avatar.light_levels[..2] == [0, 0]
        {
            continue;
        }
        if characters == MAX_CHARACTERS {
            break;
        }
        characters += 1;
        append(&mut out, avatar, catalog, &mut block);
    }
    out
}

fn append(
    out: &mut Vec<Patch>,
    avatar: &VisualAvatar,
    catalog: &Catalog,
    block: &mut impl FnMut(i32, i32, i32) -> Option<BlockId>,
) {
    let feet = avatar.position;
    // Small tolerance accommodates a presented foot just below a voxel top.
    let top = (feet.y + 0.025).floor() as i32;
    let cx = feet.x.floor() as i32;
    let cz = feet.z.floor() as i32;
    let Some(floor) = support(cx, cz, top, catalog, block) else {
        return;
    };
    let height = feet.y - floor as f32;
    if !(-0.025..MAX_HEIGHT).contains(&height) {
        return;
    }
    let fade = 1.0 - height.max(0.0) / MAX_HEIGHT;
    let opacity = 0.26 * fade * fade * if avatar.airborne { 0.6 } else { 1.0 };
    let radius = 0.58 + 0.06 * (1.0 - fade);
    let min_x = (feet.x - radius).floor() as i32;
    let max_x = (feet.x + radius).floor() as i32;
    let min_z = (feet.z - radius).floor() as i32;
    let max_z = (feet.z + radius).floor() as i32;
    let mut cells = [(0, 0, false); MAX_CELLS];
    let mut count = 0;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let dx = (feet.x - feet.x.clamp(x as f32, (x + 1) as f32)).abs();
            let dz = (feet.z - feet.z.clamp(z as f32, (z + 1) as f32)).abs();
            if dx * dx + dz * dz >= radius * radius {
                continue;
            }
            if (x == cx && z == cz) || support(x, z, top, catalog, block) == Some(floor) {
                cells[count] = (x, z, x == cx && z == cz);
                count += 1;
            }
        }
    }
    // A diagonal floor tile across two wall corners must not receive a patch.
    // Flood fill only the tiny validated footprint, not the world.
    for _ in 0..count {
        let mut changed = false;
        for i in 0..count {
            let (x, z, connected) = cells[i];
            if !connected
                && cells[..count]
                    .iter()
                    .any(|&(ox, oz, reached)| reached && (ox - x).abs() + (oz - z).abs() == 1)
            {
                cells[i].2 = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for &(x, z, connected) in &cells[..count] {
        if !connected {
            continue;
        }
        out.push(Patch {
            bounds: [
                (x as f32).max(feet.x - radius),
                (z as f32).max(feet.z - radius),
                ((x + 1) as f32).min(feet.x + radius),
                ((z + 1) as f32).min(feet.z + radius),
            ],
            center: [feet.x, feet.z, floor as f32 + 0.003, radius],
            light: [
                opacity,
                f32::from(avatar.light_levels[0].min(15)) / 15.0,
                f32::from(avatar.light_levels[1].min(15)) / 15.0,
                0.0,
            ],
        });
    }
}

/// Stop at the first occupied/unknown cell. In particular, cutouts, plants and
/// non-cube support cannot make us fall through to a different floor below.
fn support(
    x: i32,
    z: i32,
    top: i32,
    catalog: &Catalog,
    block: &mut impl FnMut(i32, i32, i32) -> Option<BlockId>,
) -> Option<i32> {
    for y in (top - 2..=top).rev() {
        let id = block(x, y, z)?;
        if id == AIR {
            continue;
        }
        let state = catalog.state(id)?;
        let top_texture = catalog.texture(state.face_texture(1, 1)?)?;
        // Alpha darkening cannot separate reflected light from self-emission.
        // Keep emissive floors and their bloom untouched, even when a custom
        // texture emits without contributing any propagated voxel blocklight.
        return (state.flags & (OPAQUE | SOLID) == (OPAQUE | SOLID)
            && state.emission == 0
            && top_texture.emission_strength == 0.0)
            .then_some(y + 1);
    }
    None
}

#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod perf;
#[cfg(test)]
mod tests;
