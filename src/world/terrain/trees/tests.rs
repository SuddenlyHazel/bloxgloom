use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

fn specimen(species: usize) -> Tree {
    let oak = if species == 9 { 0 } else { species };
    let (log, leaves) = if species == 9 {
        (WOOD, LEAVES)
    } else {
        palette().trees[species]
    };
    let (log_x, log_z) = palette().branch_wood[oak];
    Tree {
        x: 15,
        z: -1,
        ground_y: 0,
        trunk_top: 12,
        log,
        leaves,
        log_x,
        log_z,
        species,
        shape: 0,
    }
}

fn cells(tree: Tree) -> HashMap<(i64, i64, i64), BlockId> {
    let mut result = HashMap::new();
    for x in tree.x - TREE_RADIUS..=tree.x + TREE_RADIUS {
        for z in tree.z - TREE_RADIUS..=tree.z + TREE_RADIUS {
            for y in tree.ground_y + 1..=tree.trunk_top + 2 {
                if let Some(block) = tree_piece(tree, x, y, z) {
                    result.insert((x, y, z), block);
                }
            }
        }
    }
    result
}

#[test]
fn living_boughs_have_bark_faces_correct_axes_and_connected_voxels_without_added_density() {
    let catalog = crate::content::catalog();
    for species in 0..10 {
        let tree = specimen(species);
        let cells = cells(tree);
        let logs: HashSet<_> = cells
            .iter()
            .filter_map(|(position, block)| tree.is_log(*block).then_some(*position))
            .collect();
        let expected = match species {
            1 => (12, 198),
            4 => (30, 241),
            7 => (30, 319),
            _ => (24, 262),
        };
        assert_eq!(
            (logs.len(), cells.len() - logs.len()),
            expected,
            "material treatment must not inflate tree {species}"
        );
        let mut pending = VecDeque::from([(tree.x, 1, tree.z)]);
        let mut connected = HashSet::new();
        while let Some((x, y, z)) = pending.pop_front() {
            if !logs.contains(&(x, y, z)) || !connected.insert((x, y, z)) {
                continue;
            }
            for (dx, dy, dz) in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                pending.push_back((x + dx, y + dy, z + dz));
            }
        }
        assert_eq!(
            connected, logs,
            "detached rising bough in species {species}"
        );
        for ((x, y, z), block) in cells {
            assert_eq!(
                tree_piece(tree, x, y, z),
                Some(block),
                "world-coordinate repeatability"
            );
            if !tree.is_log(block) {
                continue;
            }
            let state = catalog.state(block).unwrap();
            assert!(
                crate::content::jg_rtx::is_log(catalog, block),
                "branch sustains canopy ecology"
            );
            if x == tree.x && z == tree.z {
                assert_eq!(block, tree.log, "preserve existing trunk identities");
                assert_ne!(
                    state.textures.top, state.textures.side,
                    "trunk remains cut-log material"
                );
            } else {
                let axis = if x != tree.x { "x" } else { "z" };
                assert!(
                    state
                        .properties
                        .iter()
                        .any(|(key, value)| key == "axis" && value == axis)
                );
                assert_eq!(
                    state.face_textures, [state.textures.side; 6],
                    "no sawn branch endpoint {species}"
                );
                assert!(state.key.contains("_wood["));
            }
        }
    }
}

#[test]
fn builtin_oak_tree_uses_axis_correct_bark_boughs_without_reassigning_its_trunk() {
    let seed = 0xB10C_6100;
    let tree = (-48..48)
        .flat_map(|z| (-48..48).map(move |x| (x, z)))
        .filter_map(|(x, z)| tree_anchor(x, z, seed))
        .find(|tree| tree.species == 9)
        .expect("builtin oak ecotype in deterministic region");
    assert_eq!(tree.log, WOOD);
    let catalog = crate::content::catalog();
    assert_eq!(
        catalog.state(tree.log_x).unwrap().key,
        "bloxgloom:oak_wood[axis=x]"
    );
    assert_eq!(
        catalog.state(tree.log_z).unwrap().key,
        "bloxgloom:oak_wood[axis=z]"
    );
    assert!(
        cells(tree)
            .values()
            .any(|block| *block == tree.log_x || *block == tree.log_z)
    );
}
