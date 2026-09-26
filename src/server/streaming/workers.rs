//! Bounded publication-only barriers built on the existing phase executor.
//! Results retain submission order; every accepted job is drained even when
//! another submission fails. Callers decide local disconnect vs fatal input.
use crate::server::parallel::{BatchId, JobKey, JobOutcome, PhaseExecutor};
use crate::server::simulation::{Phase, TickId};
use crate::world::ChunkKey;
use std::io;
use std::sync::mpsc;

pub(in crate::server) const WIDTH: usize = 16;
pub(in crate::server) type Job<T> = Box<dyn FnOnce() -> T + Send>;

pub(in crate::server) struct Workers {
    executor: PhaseExecutor<(), ()>,
    generation: u64,
}

impl Workers {
    pub(in crate::server) fn new(count: usize) -> io::Result<Self> {
        Ok(Self {
            executor: PhaseExecutor::new(count, WIDTH, WIDTH)
                .map_err(|error| io::Error::other(format!("publication pool: {error:?}")))?,
            generation: 0,
        })
    }

    pub(in crate::server) fn run<T: Send + 'static>(
        &mut self,
        jobs: Vec<Job<T>>,
    ) -> io::Result<Vec<Result<T, ()>>> {
        assert!(jobs.len() <= WIDTH);
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("publication generation exhausted"))?;
        let batch = BatchId::new(TickId::new(self.generation), Phase::Publish, 0);
        let mut receivers = Vec::with_capacity(jobs.len());
        for (index, job) in jobs.into_iter().enumerate() {
            let (sender, receiver) = mpsc::sync_channel(1);
            let key = JobKey::new(batch, ChunkKey { x: 0, y: 0, z: 0 }, index as u64, 0);
            #[cfg(test)]
            let coordinator = std::thread::current().id();
            // A rejected closure is dropped, closing its receiver. The barrier
            // still closes all earlier accepted jobs before reporting failure.
            let _ = self.executor.try_submit(key, move |_| {
                #[cfg(test)]
                assert_ne!(coordinator, std::thread::current().id());
                let _ = sender.send(job());
                Ok(())
            });
            receivers.push(receiver);
        }
        let results = self
            .executor
            .barrier(batch)
            .map_err(|error| io::Error::other(format!("publication barrier: {error:?}")))?;
        let mut completed = vec![false; receivers.len()];
        for owner in results.owners {
            for job in owner.jobs {
                completed[job.key.job_id as usize] =
                    matches!(job.outcome, JobOutcome::Completed(()));
            }
        }
        Ok(receivers
            .into_iter()
            .zip(completed)
            .map(|(receiver, completed)| {
                if completed {
                    receiver.try_recv().map_err(|_| ())
                } else {
                    Err(())
                }
            })
            .collect())
    }
}
