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
        let reads = commit.prepared.read_keys();
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
