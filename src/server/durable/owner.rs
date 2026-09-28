//! Registered owner payload admission through the same gate as actions.
use super::*;
use crate::server::journal::Change;
use crate::server::runtime::owner_commit::OwnerCommit;

impl Durability {
    /// On rejection the feature retains its preparation token so it can
    /// withdraw staged wake flags. On acceptance this queue owns everything.
    pub(in crate::server) fn try_stage_owner(
        &mut self,
        tick: TickId,
        changes: Vec<Change>,
        commit: OwnerCommit,
    ) -> Result<CommitBarrier, Box<(StageError, OwnerCommit)>> {
        if !commit.terrain_reads.is_current() {
            return Err(Box::new((StageError::Conflict, commit)));
        }
        let mut reads: Vec<_> = commit
            .prepared
            .read_keys()
            .into_iter()
            .chain(commit.terrain_reads.keys())
            .collect();
        if let Some(world) = &commit.world_action {
            reads.extend(world.0.changed_cells.iter().map(|cell| {
                crate::server::entities::cell_state_key(crate::server::entities::CellCoord::new(
                    cell.x, cell.y, cell.z,
                ))
            }));
        }
        let mut payload = Some(PendingPayload::Owner(commit));
        match self.try_stage_changes(tick, changes, reads, &mut payload, None) {
            Ok(true) => Ok(CommitBarrier::Through(self.next_id - 1)),
            result => {
                let error = match result {
                    Err(error) => error,
                    _ => StageError::Invalid(io::Error::other("empty owner commit")),
                };
                let Some(PendingPayload::Owner(commit)) = payload else {
                    unreachable!()
                };
                Err(Box::new((error, commit)))
            }
        }
    }
}
