use super::*;
use crate::{
    content::Catalog,
    lod::TileKey,
    world::{self, Chunk, ChunkKey},
};
const SEED: u64 = 0xB10C_6100;

#[test]
fn generated_forest_metadata_matches_source_and_crosses_negative_tile_seams() {
    let catalog = Catalog::builtins();
    let mut sampler = LodSampler::new(SEED);
    let (left, right, a, b) = (-24..-8)
        .flat_map(|z| (-12..-2).map(move |x| (x, z)))
        .find_map(|(x, z)| {
            let left = TileKey { level: 2, x, z };
            let right = TileKey {
                level: 2,
                x: x + 1,
                z,
            };
            let a = features(&mut sampler, left).unwrap();
            let b = features(&mut sampler, right).unwrap();
            a.iter()
                .any(|tree| b.contains(tree))
                .then_some((left, right, a, b))
        })
        .expect("real source crown crossing a negative tile seam");
    let tile_a = super::super::builtin_lod_tile(left, 1, SEED, &catalog).unwrap();
    let tile_b = super::super::builtin_lod_tile(right, 1, SEED, &catalog).unwrap();
    for tree in &tile_a.trees {
        if let Some(other) = tile_b
            .trees
            .iter()
            .find(|other| other.anchor == tree.anchor)
        {
            assert_eq!(
                tree, other,
                "clipped crown halves share server support/omission as well as source shape"
            );
        }
    }
    let common: Vec<_> = a.iter().filter(|tree| b.contains(tree)).collect();
    for feature in common {
        let cx = i64::from(feature.anchor[0]).div_euclid(12);
        let cz = i64::from(feature.anchor[2]).div_euclid(12);
        let source = world::terrain::tree_anchor(cx, cz, SEED).unwrap();
        assert_eq!(
            feature.anchor,
            [source.x as i32, source.ground_y as i32, source.z as i32]
        );
        assert_eq!(
            feature.trunk_height,
            i32::try_from(source.trunk_top - source.ground_y).unwrap() as u8
        );
        assert_eq!(feature.log, source.log);
        assert_eq!(feature.leaves, source.leaves);
        feature.validate(left, &catalog).unwrap();
        feature.validate(right, &catalog).unwrap();
    }
}

#[test]
fn edited_builtin_forest_uses_saved_spans_and_unchanged_snapshots_keep_proxies() {
    let catalog = Catalog::builtins();
    let (key, tile) = (-24..-8)
        .flat_map(|z| (-12..-2).map(move |x| TileKey { level: 2, x, z }))
        .find_map(|key| {
            let tile = super::super::builtin_lod_tile(key, 1, SEED, &catalog).ok()?;
            let [x, z, mx, mz] = key.bounds()?;
            tile.trees
                .iter()
                .any(|tree| {
                    tree.anchor[0] >= x
                        && tree.anchor[0] < mx
                        && tree.anchor[2] >= z
                        && tree.anchor[2] < mz
                })
                .then_some((key, tile))
        })
        .expect("source forest fixture fits bounded payload");
    let [minx, minz, maxx, maxz] = key.bounds().unwrap();
    let tree = tile
        .trees
        .iter()
        .find(|tree| {
            tree.anchor[0] >= minx
                && tree.anchor[0] < maxx
                && tree.anchor[2] >= minz
                && tree.anchor[2] < maxz
        })
        .unwrap();
    let [x, y, z] = [tree.anchor[0], tree.anchor[1] + 1, tree.anchor[2]];
    let chunk_key = ChunkKey {
        x: x.div_euclid(16),
        y: y.div_euclid(16),
        z: z.div_euclid(16),
    };
    let baseline = world::terrain::generate_blocks(chunk_key, SEED);
    let unchanged = Chunk::from_blocks(chunk_key, 1, baseline.clone());
    let same = super::super::build(key, 2, SEED, &catalog, &[unchanged]).unwrap();
    assert_eq!(
        same.trees, tile.trees,
        "cached untouched chunk doesnotremove proxies"
    );
    let mut blocks = baseline;
    blocks[Chunk::index([
        x.rem_euclid(16) as usize,
        y.rem_euclid(16) as usize,
        z.rem_euclid(16) as usize,
    ])
    .unwrap()] = world::AIR;
    let edited = super::super::build(
        key,
        3,
        SEED,
        &catalog,
        &[Chunk::from_blocks(chunk_key, 2, blocks)],
    )
    .unwrap();
    assert!(
        edited.trees.is_empty(),
        "actual saved edit uses existing authoritative span representation"
    );
    let index = ((x - minx).div_euclid(key.sample_width().unwrap())
        + 32 * (z - minz).div_euclid(key.sample_width().unwrap())) as usize;
    assert!(edited.columns[index].known(y, y + 1));
    assert!(
        !edited.columns[index]
            .spans
            .iter()
            .any(|s| s.bottom <= y && s.top > y),
        "saved air survives summary sampling"
    );
}

#[test]
fn rejected_coastal_refinements_now_fit_existing_packet_and_geometry_limits() {
    let catalog = Catalog::builtins();
    let colors = crate::render::lod::FaceColors::new(&catalog);
    for key in [
        TileKey {
            level: 3,
            x: -1,
            z: -7,
        },
        TileKey {
            level: 2,
            x: -6,
            z: -18,
        },
        TileKey {
            level: 1,
            x: -12,
            z: -34,
        },
        TileKey {
            level: 1,
            x: -13,
            z: -34,
        },
        TileKey {
            level: 4,
            x: -2,
            z: -5,
        },
    ] {
        let started = std::time::Instant::now();
        let tile = match super::super::builtin_lod_tile(key, 1, SEED, &catalog) {
            Ok(tile) => tile,
            Err(error) => {
                println!("forestLOD {key:?}: {error}");
                continue;
            }
        };
        let summary = started.elapsed();
        let message = crate::protocol::ServerMessage::LodTile {
            session: 1,
            request: 1,
            tile: tile.clone(),
        };
        let mut packet = Vec::new();
        crate::protocol::write_server_with_catalog(&mut packet, &message, &catalog).unwrap();
        let decode_start = std::time::Instant::now();
        let decoded =
            crate::protocol::read_server_with_catalog(packet.as_slice(), &catalog).unwrap();
        let decode = decode_start.elapsed();
        match decoded {
            crate::protocol::ServerMessage::LodTile { tile: decoded, .. } => {
                assert_eq!(decoded, tile)
            }
            other => panic!("unexpected {other:?}"),
        }
        let started = std::time::Instant::now();
        let mesh = crate::render::lod::mesh(&tile, &[], &catalog, &colors).unwrap();
        println!(
            "forestLOD {key:?}: trees={} spans={} packet={} meshbytes={} tris={} summary={:.2}ms decode={:.2}ms mesh={:.2}ms",
            tile.trees.len(),
            tile.columns.iter().map(|c| c.spans.len()).sum::<usize>(),
            tile.encoded_bytes(),
            mesh.byte_len(),
            (mesh.indices.len() + mesh.water_indices.len()) / 3,
            summary.as_secs_f64() * 1000.0,
            decode.as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0
        );
        assert!(tile.encoded_bytes() <= crate::lod::MAX_TILE_BYTES);
        assert!(mesh.byte_len() <= 8 * 1024 * 1024);
    }
}

#[test]
#[ignore = "CPU setup/mesh budget probe for the actual 512m cherry-grove capture"]
fn generated_forest_horizon_budget_probe() {
    let catalog = crate::content::catalog();
    let colors = crate::render::lod::FaceColors::new(catalog);
    let keys = crate::render::lod::desired_tiles(glam::Vec3::new(-679.5, 42.0, -2007.5), 512, 1, 4);
    let start = std::time::Instant::now();
    let tiles: Vec<_> = keys
        .into_iter()
        .map(|key| {
            super::super::builtin_lod_tile(key, 1, SEED, catalog)
                .unwrap_or_else(|error| panic!("{key:?}: {error}"))
        })
        .collect();
    let summary = start.elapsed();
    let (mut floating, mut maximum_gap) = (0, 0);
    for tile in &tiles {
        let [ox, oz, _, _] = tile.key.bounds().unwrap();
        let width = tile.key.sample_width().unwrap();
        for tree in &tile.trees {
            let x = (tree.anchor[0] - ox).div_euclid(width);
            let z = (tree.anchor[2] - oz).div_euclid(width);
            if !(0..32).contains(&x) || !(0..32).contains(&z) {
                continue;
            }
            let column = &tile.columns[(x + 32 * z) as usize];
            if let Some(surface) = column.spans.iter().rev().find(|s| {
                catalog.block_flags(s.state) & (crate::content::OPAQUE | crate::content::FLUID) != 0
            }) {
                let gap = tree.anchor[1] + 1 - surface.top;
                if gap > 0
                    && catalog.block_flags(surface.state) & crate::content::FLUID == 0
                    && column.known(surface.top, tree.anchor[1] + 1)
                {
                    floating += 1;
                    maximum_gap = maximum_gap.max(gap);
                }
            }
        }
    }
    println!("forest sampled-root gaps: count={floating} maximum={maximum_gap}m");
    for level in 0..=4 {
        let trees: Vec<_> = tiles
            .iter()
            .filter(|tile| tile.key.level == level)
            .flat_map(|tile| tile.trees.iter())
            .collect();
        let omitted = trees.iter().filter(|tree| tree.support_y.is_none()).count();
        let maximum = trees
            .iter()
            .filter_map(|tree| tree.support_y.map(|y| tree.anchor[1] + 1 - i32::from(y)))
            .max()
            .unwrap_or(0);
        println!(
            "forest support level={level}: descriptors={} omitted={omitted} max_extension={maximum}m",
            trees.len()
        );
        assert!(maximum <= 3);
    }
    let start = std::time::Instant::now();
    let mut bytes = 0;
    let mut triangles = 0;
    for tile in &tiles {
        let [x, z, mx, mz] = tile.key.bounds().unwrap();
        let neighbors: Vec<_> = tiles
            .iter()
            .filter(|other| {
                let [ox, oz, omx, omz] = other.key.bounds().unwrap();
                ((mx == ox || omx == x) && z < omz && oz < mz)
                    || ((mz == oz || omz == z) && x < omx && ox < mx)
            })
            .collect();
        let mesh = crate::render::lod::mesh(tile, &neighbors, catalog, &colors).unwrap();
        assert!(mesh.byte_len() <= 8 * 1024 * 1024);
        bytes += mesh.byte_len();
        triangles += (mesh.indices.len() + mesh.water_indices.len()) / 3;
    }
    println!(
        "forest horizon: tiles={} unavailable=0 trees={} packetbytes={} maxspans={} maxpacket={} meshbytes={bytes} triangles={triangles} summary={:.2}ms meshing={:.2}ms",
        tiles.len(),
        tiles.iter().map(|tile| tile.trees.len()).sum::<usize>(),
        tiles.iter().map(|tile| tile.encoded_bytes()).sum::<usize>(),
        tiles
            .iter()
            .map(|tile| tile.columns.iter().map(|c| c.spans.len()).sum::<usize>())
            .max()
            .unwrap(),
        tiles.iter().map(|tile| tile.encoded_bytes()).max().unwrap(),
        summary.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64() * 1000.0
    );
}
