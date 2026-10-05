//! Fair pipelined admission and bounded delivery, independent of queue heads.
use super::*;
use crate::lod::loading::ms;
const DELIVERY_PER_CLIENT: usize = 4;
const DELIVERY_PER_TICK: usize = 8;
const DELIVERY_BYTES_PER_CLIENT: usize = 128 * 1024;

pub(super) fn poll(state: &mut State) {
    let mut ids: Vec<_> = state.lod.clients.keys().copied().collect();
    ids.sort_unstable();
    let pivot = ids.partition_point(|id| *id <= state.lod.cursor);
    ids.rotate_left(pivot);
    // Inspect gameplay backlog before adding this tick's bounded LOD burst.
    // Rechecking the old 32-KiB threshold after each tile would serialize even
    // an empty socket queue. Outbound's aggregate/peer caps still apply.
    let mut allowance: HashMap<_, _> = ids
        .iter()
        .filter_map(|id| {
            let client = state.clients.get(id)?;
            let queued = client.sender.snapshot();
            (queued.queued_bytes <= 32 * 1024 && queued.queued_frames <= 8)
                .then_some((*id, (0usize, 0usize)))
        })
        .collect();
    let mut delivered = 0;
    for _ in 0..MAX_CLIENT_REQUESTS {
        let mut progress = false;
        for &id in &ids {
            let Some((count, bytes)) = allowance.get_mut(&id) else {
                continue;
            };
            let client = &state.clients[&id];
            let interest = state.lod.clients.get_mut(&id).unwrap();
            if *count < DELIVERY_PER_CLIENT && delivered < DELIVERY_PER_TICK {
                let ready = interest
                    .requests
                    .iter()
                    .enumerate()
                    .find_map(|(index, request)| {
                        let tile = state.lod.cache.get(&request.key)?;
                        // Conservative header allowance without cloning a rejected frame.
                        let size = tile.encoded_bytes() + 64;
                        (*bytes + size <= DELIVERY_BYTES_PER_CLIENT)
                            .then_some((index, *request, tile, size))
                    });
                if let Some((index, request, tile, size)) = ready {
                    if client
                        .sender
                        .try_send(ServerMessage::LodTile {
                            session: client.action_epoch,
                            request: request.id,
                            tile: tile.clone(),
                        })
                        .is_err()
                    {
                        *count = DELIVERY_PER_CLIENT;
                        continue;
                    }
                    tracing::debug!(target: "bloxgloom::lod_loading", client=id, request=request.id, key=?request.key,
                        revision=tile.revision, request_to_enqueue_ms=ms(request.accepted.elapsed()), bytes=size, "LOD delivery queued");
                    interest.requests.remove(index);
                    *count += 1;
                    *bytes += size;
                    delivered += 1;
                    progress = true;
                    state.lod.cursor = id;
                }
            }
            // Ready frames and pending work cannot block admission behind them.
            if state.lod.pending.len() >= MAX_PENDING {
                continue;
            }
            let Some(request) = interest
                .requests
                .iter()
                .find(|request| {
                    !state.lod.pending.contains_key(&request.key)
                        && !state.lod.cache.contains_key(&request.key)
                })
                .copied()
            else {
                continue;
            };
            let captured = Instant::now();
            let bounds = request.key.bounds().unwrap();
            let snapshots = state.world.lod_overlays(bounds).and_then(|overlays| {
                state
                    .world
                    .lod_resident(bounds)
                    .map(|resident| (overlays, resident))
            });
            let Ok((overlays, resident)) = snapshots else {
                interest.requests.retain(|r| r.id != request.id);
                let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                    session: client.action_epoch,
                    request: request.id,
                    key: request.key,
                });
                progress = true;
                continue;
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            let revision = state.lod.revision;
            let children = request.key.children().and_then(|keys| {
                let values: Vec<_> = keys
                    .iter()
                    .map(|key| state.lod.cache.get(key).cloned())
                    .collect::<Option<Vec<_>>>()?;
                values.try_into().ok()
            });
            let job = worker::Job {
                key: request.key,
                revision,
                overlays,
                resident,
                children,
                requested_at: Instant::now(),
                cancelled: cancelled.clone(),
            };
            if state.lod.jobs.as_ref().unwrap().try_send(job).is_ok() {
                state.lod.pending.insert(request.key, (revision, cancelled));
                tracing::debug!(target: "bloxgloom::lod_loading", client=id, request=request.id, key=?request.key,
                    admission_wait_ms=ms(captured.saturating_duration_since(request.accepted)),
                    capture_ms=ms(captured.elapsed()), revision, "LOD build admitted");
                state.lod.cursor = id;
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
}
