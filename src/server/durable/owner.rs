//! Registered owner payload admission through the same gate as actions.
use super::*;
use crate::server::journal::Change;
use crate::server::runtime::owner_commit::OwnerCommit;

#[cfg(test)]
mod tests;

impl Durability {
    /// On rejection the feature retains its preparation token so it can
    /// withdraw staged wake flags. On acceptance this queue owns everything.
    pub(in crate::server) fn try_stage_owner(
        &mut self,
        tick: TickId,
        mut changes: Vec<Change>,
        mut commit: OwnerCommit,
    ) -> Result<CommitBarrier, Box<(StageError, OwnerCommit)>> {
        if !commit.terrain_reads.is_current()
            || commit
                .world_action
                .as_ref()
                .is_some_and(|world| !world.0.terrain_reads.is_current())
        {
            return Err(Box::new((StageError::Conflict, commit)));
        }
        let mut entity_permit = None;
        let mut frontier = None;
        if let Some(entities) = commit
            .world_action
            .as_mut()
            .and_then(|world| world.0.entities.as_mut())
        {
            let prepare = (|| {
                entities
                    .assign_publication(self.entity_publication_frontier)
                    .map_err(|error| StageError::Invalid(io::Error::other(error)))?;
                frontier = Some(
                    entities
                        .publication_frontier()
                        .map_err(|error| StageError::Invalid(io::Error::other(error)))?,
                );
                // Only the publication revision is rebased; real participant
                // preimages and read dependencies retain their captured values.
                changes.retain(|change| {
                    change.key.domain != crate::server::entities::ENTITY_REVISION_DOMAIN
                });
                changes.extend(
                    entities
                        .changes()
                        .iter()
                        .filter(|change| {
                            change.key.domain == crate::server::entities::ENTITY_REVISION_DOMAIN
                        })
                        .cloned(),
                );
                entity_permit = Some(
                    self.entity_mirror
                        .try_reserve_durable()
                        .map_err(StageError::Invalid)?
                        .ok_or(StageError::Full)?,
                );
                Ok::<(), StageError>(())
            })();
            if let Err(error) = prepare {
                return Err(Box::new((error, commit)));
            }
        }
        let mut reads: Vec<_> = commit
            .prepared
            .read_keys()
            .into_iter()
            .chain(commit.terrain_reads.keys())
            .collect();
        if let Some(world) = &commit.world_action {
            reads.extend(world.0.terrain_reads.keys());
            if let Some(entities) = &world.0.entities {
                reads.extend(entities.read_keys().cloned());
            }
            reads.extend(world.0.changed_cells.iter().map(|cell| {
                crate::server::entities::cell_state_key(crate::server::entities::CellCoord::new(
                    cell.x, cell.y, cell.z,
                ))
            }));
        }
        let mut payload = Some(PendingPayload::Owner(commit));
        match self.try_stage_changes(tick, changes, reads, &mut payload, entity_permit) {
            Ok(true) => {
                if let Some(frontier) = frontier {
                    self.entity_publication_frontier = frontier;
                }
                Ok(CommitBarrier::Through(self.next_id - 1))
            }
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
