//! Package-owned profile values use the existing revisioned owner WAL domain.
use crate::server::{
    parallel::{OwnerData, OwnerKey},
    registry::{OwnerPartition, SystemId},
    runtime::{
        owner_codec::{OwnerCodecError, OwnerValueCodec},
        owner_durable::{OwnerSystemConfig, OwnerWrite},
    },
};
use bloxgloom_host_api::players::{Registration, State};
use std::{io, sync::Arc};
struct Codec {
    max: usize,
}
impl OwnerValueCodec for Codec {
    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        let state = value.get::<State>().ok_or(OwnerCodecError::InvalidData)?;
        if state.data.len() > self.max || state.public_data.len() > 1024 {
            return Err(OwnerCodecError::InvalidData);
        }
        let mut bytes = (state.data.len() as u16).to_le_bytes().to_vec();
        bytes.extend((state.public_data.len() as u16).to_le_bytes());
        bytes.extend(&state.data);
        bytes.extend(&state.public_data);
        Ok(bytes)
    }
    fn decode(&self, bytes: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        if bytes.len() < 4 {
            return Err(OwnerCodecError::InvalidData);
        }
        let private = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let public = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        if private > self.max || public > 1024 || bytes.len() != 4 + private + public {
            return Err(OwnerCodecError::InvalidData);
        }
        Ok(OwnerData::new(State {
            data: bytes[4..4 + private].to_vec(),
            public_data: bytes[4 + private..].to_vec(),
        }))
    }
}
pub(in crate::server) fn config(reg: &Registration) -> io::Result<OwnerSystemConfig> {
    OwnerSystemConfig::new(
        SystemId::new(&reg.key).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid player service key")
        })?,
        Arc::new(Codec {
            max: usize::from(reg.max_state_bytes),
        }),
        1,
        usize::from(reg.max_state_bytes) + 1028,
        OwnerPartition::Profile,
    )
}
/// Stage the flag/reward state together, without publishing a cell before sync.
pub(in crate::server) fn prepare(
    runtime: &crate::server::runtime::systems::SystemRuntime,
    reg: &Registration,
    profile: u128,
    value: State,
    due_tick: Option<u64>,
) -> io::Result<Vec<crate::server::journal::Change>> {
    let system = SystemId::new(&reg.key)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid player service key"))?;
    let owner = OwnerKey::Profile(profile);
    let value = OwnerData::new(value);
    if let Some((revision, _)) = runtime.owner_snapshot(&system, owner) {
        let wave = runtime
            .prepare_owner_wave(
                &system,
                vec![OwnerWrite {
                    owner,
                    reads: vec![(owner, revision)],
                    value,
                    due_tick,
                }],
            )
            .map_err(|e| e.io())?;
        Ok(wave.changes().to_vec())
    } else {
        runtime
            .stage_profile_insert(&system, profile, &value, due_tick)
            .map(|change| vec![change])
    }
}

/// Ordinary actions retain the captured deadline and revision. Every queried
/// cell (including absence) is reserved by the planner until receipt.
pub(in crate::server) fn prepare_writes(
    runtime: &crate::server::runtime::systems::SystemRuntime,
    catalog: &crate::content::Catalog,
    pending_inserts: usize,
    writes: std::collections::BTreeMap<(String, u128), bloxgloom_host_api::gameplay::ProfileCell>,
) -> io::Result<Vec<crate::server::journal::Change>> {
    let inserts = writes.values().filter(|cell| !cell.initialized).count();
    if inserts > 0 && !runtime.has_profile_insert_room(pending_inserts.saturating_add(inserts - 1))
    {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "profile cell capacity exhausted",
        ));
    }
    let mut changes = Vec::new();
    for ((key, profile), cell) in writes {
        let internal;
        let reg = if key == bloxgloom_host_api::player_modifiers::PROFILE_SYSTEM {
            internal = super::modifiers::registration();
            &internal
        } else {
            catalog
                .player_lifecycles()
                .find(|reg| reg.key == key)
                .ok_or_else(|| io::Error::other("unregistered player service output"))?
        };
        let system = SystemId::new(&key).map_err(|_| io::Error::other("invalid player service"))?;
        if runtime
            .owner_snapshot(&system, OwnerKey::Profile(profile))
            .map(|(revision, _)| revision)
            != cell.initialized.then_some(cell.revision)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "stale profile state",
            ));
        }
        changes.extend(prepare(runtime, reg, profile, cell.state, cell.next_tick)?);
    }
    Ok(changes)
}
