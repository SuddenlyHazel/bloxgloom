//! Rain hits only the first known, exposed voxel face in each nearby column.
use crate::{
    audio::rain_scene::{RainMaterial, RainScene, RainTile},
    content::{BlockStateId, Catalog},
    world,
};
use glam::Vec3;

fn material(catalog: &Catalog, id: BlockStateId) -> Option<RainMaterial> {
    let block = catalog.block(id)?;
    if !block.solid {
        return None;
    }
    Some(if block.cutout {
        RainMaterial::Leaf
    } else if block.key == "bloxgloom:wood" {
        RainMaterial::Wood
    } else if [
        world::GRASS,
        world::DIRT,
        world::SAND,
        world::SNOW,
        world::MOSS,
        world::GRAVEL,
    ]
    .contains(&id)
    {
        RainMaterial::Dirt
    } else {
        RainMaterial::Concrete
    })
}

pub(super) fn sample(
    eye: Vec3,
    top: i32,
    catalog: &Catalog,
    mut block: impl FnMut(i32, i32, i32) -> Option<BlockStateId>,
) -> RainScene {
    let mut scene = RainScene::default();
    let ox = eye.x.floor() as i32 - 8;
    let oz = eye.z.floor() as i32 - 8;
    let bottom = (eye.y.floor() as i32).saturating_sub(16);
    for z in oz..oz + 16 {
        for x in ox..ox + 16 {
            for y in (bottom..=top).rev() {
                let Some(id) = block(x, y, z) else {
                    break;
                };
                let Some(material) = material(catalog, id) else {
                    continue;
                };
                scene.tiles.push(RainTile {
                    centre: [x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5],
                    material,
                    normal: [0.0; 2],
                });
                // Current weather travels toward +X/+Z. These windward faces
                // receive driving rain only when their neighbouring cell is known air.
                for (dx, dz, normal) in [(-1, 0, [-1.0, 0.0]), (0, -1, [0.0, -1.0])] {
                    if block(x + dx, y, z + dz)
                        .is_some_and(|id| catalog.block_flags(id) & crate::content::SOLID == 0)
                    {
                        scene.tiles.push(RainTile {
                            centre: [
                                x as f32 + 0.5 + dx as f32 * 0.5,
                                y as f32 + 0.5,
                                z as f32 + 0.5 + dz as f32 * 0.5,
                            ],
                            material,
                            normal,
                        });
                    }
                }
                break;
            }
        }
    }
    scene
}

#[cfg(test)]
mod tests;
