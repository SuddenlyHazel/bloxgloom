use super::*;
use crate::lod::{Column, TILE_COLUMNS};

fn state() -> State {
    let mut state = State::new(Arc::new(Catalog::builtins()));
    state.session = 7;
    state.configured = Some(512);
    state
}
fn tile(key: TileKey, revision: u64) -> LodTile {
    LodTile {
        key,
        revision,
        columns: vec![Column::default(); TILE_COLUMNS],
        geometric_error: 0,
    }
}
#[test]
fn obsolete_requests_and_other_sessions_cannot_install_tiles() {
    let mut state = state();
    let key = TileKey::containing(2, -1, 0).unwrap();
    state.wanted.push(key);
    state.requests.insert(
        key,
        Request {
            id: 10,
            started: Instant::now(),
        },
    );
    state
        .mesh_retry
        .insert(key, Instant::now() + Duration::from_secs(60));
    state.accept(8, 10, tile(key, 1));
    state.accept(7, 9, tile(key, 1));
    assert!(state.tiles.is_empty());
    assert!(state.requests.contains_key(&key));
    state.accept(7, 10, tile(key, 1));
    assert_eq!(state.tiles.len(), 1);
    assert!(!state.mesh_retry.contains_key(&key));
    state.retire();
}
#[test]
fn invalidation_rejects_old_builds_but_keeps_displayed_replicas() {
    let mut state = state();
    let key = TileKey {
        level: 2,
        x: 0,
        z: 0,
    };
    state.wanted.push(key);
    state.tiles.insert(key, Arc::new(tile(key, 1)));
    state.builds.insert(key, 12);
    state.requests.insert(
        key,
        Request {
            id: 2,
            started: Instant::now(),
        },
    );
    state.invalidate_all(7, 3);
    assert_eq!(state.tiles[&key].revision, 1);
    assert!(state.builds.is_empty());
    assert!(state.requests.is_empty());
    state.requests.insert(
        key,
        Request {
            id: 3,
            started: Instant::now(),
        },
    );
    state.accept(7, 3, tile(key, 2));
    assert_eq!(state.tiles[&key].revision, 1);
    state.retire();
}
#[test]
fn teleport_reset_cancels_requests_without_reusing_request_identity() {
    let mut state = state();
    let key = TileKey {
        level: 3,
        x: 0,
        z: 0,
    };
    state.wanted.push(key);
    state.requests.insert(
        key,
        Request {
            id: 4,
            started: Instant::now(),
        },
    );
    state.next_request = 5;
    state.reset();
    state.accept(7, 4, tile(key, 1));
    assert!(state.tiles.is_empty());
    assert_eq!(state.next_request, 5);
    state.retire();
}

#[test]
fn mixed_detail_boundary_neighbors_are_remeshed_without_overlapping_ancestors() {
    let coarse = TileKey {
        level: 2,
        x: 0,
        z: 0,
    };
    assert!(neighboring(
        coarse,
        TileKey {
            level: 1,
            x: 2,
            z: 0
        }
    ));
    assert!(!neighboring(
        coarse,
        TileKey {
            level: 1,
            x: 0,
            z: 0
        }
    ));
    assert!(!neighboring(
        coarse,
        TileKey {
            level: 1,
            x: 2,
            z: 2
        }
    ));
}
