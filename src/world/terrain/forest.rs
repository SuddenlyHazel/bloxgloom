//! Source tree descriptor projection; no independent tree generation equations.
use super::trees::Tree;
use crate::lod::TreeFeature;

pub(in crate::world) fn feature(tree: Tree) -> Option<TreeFeature> {
    let (species, shape) = tree.presentation_shape();
    Some(TreeFeature {
        anchor: [
            i32::try_from(tree.x).ok()?,
            i32::try_from(tree.ground_y).ok()?,
            i32::try_from(tree.z).ok()?,
        ],
        support_y: i16::try_from(tree.ground_y + 1).ok(),
        trunk_height: u8::try_from(tree.trunk_top - tree.ground_y).ok()?,
        log: tree.log,
        leaves: tree.leaves,
        branch_x: tree.presentation_bark().0,
        branch_z: tree.presentation_bark().1,
        species,
        shape,
    })
}
