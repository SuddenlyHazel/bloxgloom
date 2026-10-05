//! One retained completion applies backpressure instead of a timed remesh retry.
use super::*;
use crate::{
    lod::loading::ms,
    render::lod::{Mesh, UploadError},
};

pub(super) struct Pending {
    generation: u64,
    mesh: Box<Mesh>,
}

pub(super) fn retry(
    state: &mut State,
    enqueue: impl FnMut(Mesh) -> Result<(), UploadError>,
) -> bool {
    let Some(pending) = state.ready_upload.take() else {
        return true;
    };
    let key = pending.mesh.key;
    if state.builds.get(&key) != Some(&pending.generation)
        || !state.wanted.contains(&key)
        || pending.mesh.revision < state.minimum.get(&key).copied().unwrap_or(0)
    {
        if state.builds.get(&key) == Some(&pending.generation) {
            state.builds.remove(&key);
        }
        return true;
    }
    offer(state, pending.generation, *pending.mesh, enqueue)
}

pub(super) fn offer(
    state: &mut State,
    generation: u64,
    mesh: Mesh,
    mut enqueue: impl FnMut(Mesh) -> Result<(), UploadError>,
) -> bool {
    let key = mesh.key;
    let trace = mesh.loading;
    match enqueue(mesh) {
        Ok(()) => {
            state.builds.remove(&key);
            tracing::debug!(target: "bloxgloom::lod_loading", ?key, generation,
                upload_admission_ms=trace.map(|t| ms(t.queued.elapsed())), "LOD upload admitted");
            true
        }
        Err(UploadError::QueueFull(mesh)) => {
            state.ready_upload = Some(Pending { generation, mesh });
            false
        }
        Err(UploadError::Budget(mesh)) => {
            // A tile that cannot fit residency must not block smaller tiles.
            // Movement/eviction can free space later; keep the coarse fallback.
            state.builds.remove(&key);
            state.pending_mesh.insert(key);
            state
                .mesh_retry
                .insert(key, Instant::now() + Duration::from_secs(2));
            tracing::debug!(target: "bloxgloom::lod_loading", ?key, generation, bytes=mesh.byte_len(), "LOD upload residency budget deferred");
            true
        }
    }
}

#[cfg(test)]
mod tests;
