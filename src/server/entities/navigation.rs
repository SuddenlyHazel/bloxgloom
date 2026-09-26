//! Bounded local A* for ground walkers. Searches execute inside entity workers;
//! their voxel reads are covered by the existing tick input revision fence.
//! No cache survives a terrain revision: routes are short and replanned against
//! the current view, so edits cannot leave stale waypoints driving locomotion.
use super::{EntityError, locomotion::Body};
use crate::server::voxel_view::VoxelView;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

const MAX_NODES: usize = 128;
const RADIUS: i32 = 8;

#[derive(Debug, PartialEq)]
pub(super) enum Route {
    Arrived,
    Next([f32; 3]),
    Unreachable,
    BudgetExhausted,
}

/// Level-ground capability: no implicit jumping, climbing, or unsafe drops.
/// Other locomotion modes can supply separate graphs without changing A*'s
/// entity scheduling, persistence, or commit contract.
pub(super) fn route(
    view: &VoxelView,
    body: Body,
    position: [f32; 3],
    goal: [i32; 2],
) -> Result<Route, EntityError> {
    let start = [position[0].floor() as i32, position[2].floor() as i32];
    let point = |node: [i32; 2]| [node[0] as f32 + 0.5, position[1], node[1] as f32 + 0.5];
    if !super::locomotion::valid_position(position)
        || !super::locomotion::valid_position(point(goal))
    {
        return Ok(Route::Unreachable);
    }
    if start == goal {
        return Ok(
            if glam::Vec3::from_array(position).distance(glam::Vec3::from_array(point(goal))) < 0.02
            {
                Route::Arrived
            } else {
                Route::Next(point(goal))
            },
        );
    }
    let distance = |a: [i32; 2], b: [i32; 2]| a[0].abs_diff(b[0]) + a[1].abs_diff(b[1]);
    if distance(start, goal) > RADIUS as u32 {
        return Ok(Route::Unreachable);
    }
    if !body.clear(view, point(goal))? || !body.supported(view, point(goal))? {
        return Ok(Route::Unreachable);
    }
    let mut open = BinaryHeap::from([Reverse((distance(start, goal), 0u32, start))]);
    let mut visited = BTreeMap::from([(start, (0u32, start))]);
    let mut expanded = 0;
    while let Some(Reverse((_, cost, node))) = open.pop() {
        if visited[&node].0 != cost {
            continue;
        }
        if node == goal {
            let mut next = goal;
            while visited[&next].1 != start {
                next = visited[&next].1;
            }
            return Ok(Route::Next(point(next)));
        }
        if expanded == MAX_NODES {
            return Ok(Route::BudgetExhausted);
        }
        expanded += 1;
        for offset in [[0, 1], [1, 0], [0, -1], [-1, 0]] {
            let next = [node[0] + offset[0], node[1] + offset[1]];
            if distance(start, next) > RADIUS as u32
                || visited.get(&next).is_some_and(|(old, _)| *old <= cost + 1)
            {
                continue;
            }
            let from = if node == start { position } else { point(node) };
            if !body.walk_edge(view, from, point(next))? {
                continue;
            }
            // Bound memory as well as expansion, including the frontier.
            if !visited.contains_key(&next) && visited.len() == MAX_NODES {
                return Ok(Route::BudgetExhausted);
            }
            visited.insert(next, (cost + 1, node));
            open.push(Reverse((cost + 1 + distance(next, goal), cost + 1, next)));
        }
    }
    Ok(Route::Unreachable)
}

#[cfg(test)]
mod tests;
