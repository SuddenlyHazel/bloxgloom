use super::*;
use crate::{
    content::Catalog,
    render::trace::scene::{Chunk, Scene},
    world,
};
use std::sync::Arc;
#[test]
fn worker_static_tags_preserve_botanical_and_unknown_motion_allowance() {
    let catalog = Catalog::builtins();
    let stone = catalog
        .textures()
        .iter()
        .find(|t| t.key == "bloxgloom:stone")
        .unwrap();
    let leaves = catalog
        .textures()
        .iter()
        .find(|t| t.key == "bloxgloom:jg_cherry_leaves")
        .unwrap();
    assert!(stationary(Some(stone)));
    assert!(!stationary(Some(leaves)));
    assert!(!stationary(None));
    let mut package = stone.clone();
    package.key = "package:industrial".into();
    assert!(
        !stationary(Some(&package)),
        "unclassified package geometry stays conservative"
    );
    let mut blocks = vec![world::AIR; world::CHUNK_SIZE.pow(3)];
    blocks[world::Chunk::index([5, 5, 5]).unwrap()] = world::STONE;
    let chunk = world::Chunk::from_blocks(world::ChunkKey { x: 0, y: 0, z: 0 }, 0, blocks);
    let mesh = crate::render::mesh::mesh_chunk(&chunk);
    assert!(mesh.trace.triangles.iter().all(|t| t.normal[3] == -1.0));
    let legacy = Scene::build_with_bounds([mesh.trace.clone()], false);
    let tight = Scene::build_with_bounds([mesh.trace.clone()], true);
    assert!(legacy.nodes[0].min[0] < 4.9 && tight.nodes[0].min[0] > 4.99);
    assert_eq!(legacy.triangles.len(), tight.triangles.len());
    let mut unknown = mesh.trace.triangles.clone();
    for t in &mut unknown {
        t.normal[3] = 0.0;
    }
    let unknown = Scene::build_with_bounds(
        [Arc::new(Chunk {
            water: None,
            coarse_water: None,

            triangles: unknown,
            key: None,
        })],
        true,
    );
    assert_eq!(legacy.nodes[0].min, unknown.nodes[0].min);
    assert_eq!(legacy.nodes[0].max, unknown.nodes[0].max);
    for t in &mesh.trace.triangles {
        assert_eq!(padding(t, false), 0.12);
        assert_eq!(padding(t, true), 0.0001);
    }
}
