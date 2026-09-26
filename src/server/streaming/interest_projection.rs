//! Pure interest and legacy drop preparation. Only immutable page handles and
//! subscription captures cross the worker boundary; all pins stay coordinator-owned.
use super::workers::{Job, WIDTH};
use super::*;

struct Input {
    id: u64,
    center: ChunkKey,
    radius: u8,
    position: [f32; 3],
    drop_revision: u64,
    sent: Arc<HashSet<ChunkKey>>,
    drops: Option<Vec<crate::server::entities::MobilePage>>,
    previous: Vec<DroppedItem>,
    now_ms: u64,
}

struct Prepared {
    input: Input,
    released: Vec<ChunkKey>,
    candidates: Vec<ChunkKey>,
    drops: Option<Vec<DroppedItem>>,
    drop_frame: Option<Arc<crate::server::outbound::SharedMessage>>,
}

pub(super) fn prepare(
    state: &mut State,
    ids: &[u64],
    selection: &mut snapshots::Selection,
) -> io::Result<()> {
    for batch in ids.chunks(WIDTH) {
        let mut jobs: Vec<Job<Prepared>> = Vec::new();
        let mut targets = Vec::new();
        let mut unavailable = Vec::new();
        for &id in batch {
            let Some(client) = state.clients.get(&id) else {
                continue;
            };
            let position = client.position();
            let anchor = position.map(|v| (v / 4.0).floor() as i32);
            let dirty = client.last_drops_revision != state.drop_revision
                || client.last_drop_anchor != anchor;
            let drops = if dirty {
                match drops::capture_nearby(&state.entities, position) {
                    Ok(pages) => Some(pages),
                    Err(()) => {
                        unavailable.push(id);
                        continue;
                    }
                }
            } else {
                None
            };
            let input = Input {
                id,
                center: client.center,
                radius: client.radius,
                position,
                drop_revision: state.drop_revision,
                sent: client.sent.capture(),
                drops,
                previous: if dirty {
                    client.last_sent_drops.clone()
                } else {
                    Vec::new()
                },
                now_ms: drops::unix_ms(),
            };
            targets.push(id);
            jobs.push(Box::new(move || {
                let mut input = input;
                let mut kept = (*input.sent).clone();
                let mut released = Vec::new();
                kept.retain(|key| {
                    if inside_view(*key, input.center, i64::from(input.radius)) {
                        true
                    } else {
                        released.push(*key);
                        false
                    }
                });
                let candidates = interest::nearest_unsent(
                    input.center,
                    input.radius,
                    &kept,
                    MAX_PREFETCH_CANDIDATES,
                );
                let drops = input
                    .drops
                    .take()
                    .map(|pages| drops::project_nearby(pages, input.position, input.now_ms));
                let changed = drops
                    .as_ref()
                    .is_some_and(|items| !same_drop_positions(items, &input.previous));
                let drop_frame = if changed {
                    Some(crate::server::outbound::SharedMessage::new(
                        ServerMessage::Drops {
                            revision: input.drop_revision,
                            items: drops.as_ref().expect("changed drop projection").clone(),
                        },
                    ))
                } else {
                    None
                };
                Prepared {
                    input,
                    released,
                    candidates,
                    drops,
                    drop_frame,
                }
            }));
        }
        let results = state.publication_workers.run(jobs)?;
        for (id, result) in targets.into_iter().zip(results) {
            match result {
                Ok(prepared) => apply(state, prepared, selection)?,
                Err(()) => unavailable.push(id),
            }
        }
        // An unrepresentable region or failed worker closes this session rather
        // than endlessly sending misleading empty drop frames.
        for id in unavailable {
            disconnect(state, id);
        }
    }
    Ok(())
}

fn apply(
    state: &mut State,
    prepared: Prepared,
    selection: &mut snapshots::Selection,
) -> io::Result<()> {
    let Prepared {
        input,
        released,
        candidates,
        drops,
        drop_frame,
    } = prepared;
    let Some(client) = state.clients.get_mut(&input.id) else {
        return Ok(());
    };
    if client.center != input.center
        || client.radius != input.radius
        || client.position() != input.position
        || state.drop_revision != input.drop_revision
        || !client.sent.matches(&input.sent)
    {
        return Ok(());
    }
    let id = input.id;
    let anchor = input.position.map(|v| (v / 4.0).floor() as i32);
    let revision = input.drop_revision;
    drop(input); // release immutable map before mutation (no COW map copy)
    if let Some(frame) = drop_frame
        && client.sender.try_send_shared(frame).is_err()
    {
        disconnect(state, id);
        return Ok(());
    }
    if let Some(items) = drops {
        client.last_sent_drops = items;
        client.last_drops_revision = revision;
        client.last_drop_anchor = anchor;
    }
    for key in released {
        client.sent.remove(&key);
        client.sent_epochs.remove(&key);
        client.sent_block_versions.remove(&key);
        client.sent_entity_revisions.remove(&key);
        let released = state.world.unpin_resident_chunk(key);
        debug_assert!(released);
    }
    super::stream_candidates(state, id, candidates, selection)
}

pub(super) fn disconnect(state: &mut State, id: u64) {
    if let Some(client) = state.clients.get(&id) {
        let _ = client.socket.shutdown(Shutdown::Both);
    }
    state.remove_client(id);
}

pub(super) fn reduce_view_under_pressure(state: &mut State) -> io::Result<()> {
    if state.world.can_admit_chunk() {
        return Ok(());
    }
    let mut ids: Vec<_> = state.clients.keys().copied().collect();
    ids.sort_unstable();
    let mut best = None;
    for batch in ids.chunks(WIDTH) {
        let mut jobs: Vec<Job<_>> = Vec::new();
        let mut targets = Vec::new();
        for &id in batch {
            let client = &state.clients[&id];
            if client.radius <= MIN_VIEW_DISTANCE {
                continue;
            }
            let sent = client.sent.capture();
            let center = client.center;
            let radius = client.radius;
            targets.push(id);
            jobs.push(Box::new(move || {
                let released: Vec<_> = sent
                    .iter()
                    .copied()
                    .filter(|key| !inside_view(*key, center, i64::from(radius - 1)))
                    .collect();
                (
                    (!released.is_empty(), radius, released.len(), sent.len(), id),
                    released,
                )
            }));
        }
        for (id, result) in targets
            .into_iter()
            .zip(state.publication_workers.run(jobs)?)
        {
            match result {
                Ok(candidate) if best.as_ref().is_none_or(|(score, _)| candidate.0 > *score) => {
                    best = Some(candidate)
                }
                Ok(_) => {}
                Err(()) => disconnect(state, id),
            }
        }
    }
    if let Some(((.., id), released)) = best {
        let Some(client) = state.clients.get_mut(&id) else {
            return Ok(());
        };
        client.radius -= 1;
        if !client.enqueue(ServerMessage::ViewDistance {
            radius: client.radius,
        }) {
            disconnect(state, id);
            return Ok(());
        }
        for key in released {
            client.sent.remove(&key);
            client.sent_epochs.remove(&key);
            client.sent_block_versions.remove(&key);
            client.sent_entity_revisions.remove(&key);
            let unpinned = state.world.unpin_resident_chunk(key);
            debug_assert!(unpinned);
        }
    }
    Ok(())
}
