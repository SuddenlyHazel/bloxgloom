//! Fixed-step scheduling primitives for the authoritative server.
//!
//! This module intentionally contains no world, socket, or persistence access.
//! The server coordinator owns this clock and drains queues at phase barriers;
//! networking and filesystem work stay outside the tick path.

use std::collections::HashSet;
use std::time::Duration;

#[cfg(test)]
#[path = "simulation/tests.rs"]
mod tests;

pub(super) const FIXED_STEP: Duration = Duration::from_millis(20);

/// The first emitted tick is 1; tick 0 represents the state before simulation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TickId(u64);

impl TickId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

/// Stable ordering key shared by commands and server-produced effects.
///
/// Field order is significant: sorting uses tick, then source, then sequence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct OrderKey {
    tick: TickId,
    source: u64,
    sequence: u64,
}

impl OrderKey {
    pub(super) const fn new(tick: TickId, source: u64, sequence: u64) -> Self {
        Self {
            tick,
            source,
            sequence,
        }
    }

    pub(super) const fn tick(self) -> TickId {
        self.tick
    }
}

/// Ordered authoritative tick stages. Keep this order aligned with the server
/// simulation design; each phase sees writes committed at the prior barrier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Phase {
    InputAuthorization,
    DurableActions,
    Simulation,
    InteractionCommit,
    Publish,
}

impl Phase {
    pub(super) const ALL: [Self; 5] = [
        Self::InputAuthorization,
        Self::DurableActions,
        Self::Simulation,
        Self::InteractionCommit,
        Self::Publish,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ClockError {
    TickIdExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TickAdvance {
    /// Consecutive tick IDs the coordinator should execute, in order.
    pub(super) ticks: Vec<TickId>,
    /// Elapsed simulation time still waiting after this bounded catch-up batch.
    pub(super) backlog: Duration,
    /// Whole fixed steps waiting in `backlog` (the fractional remainder is kept
    /// in `backlog` but not counted here).
    pub(super) backlog_ticks: u64,
}

/// Converts elapsed wall time into fixed-step tick IDs without skipping work.
///
/// Callers choose a catch-up limit per coordinator pass. Excess time remains
/// queued as backlog, making lag visible to metrics/backpressure policy.
#[derive(Clone, Debug)]
pub(super) struct FixedStepClock {
    last_tick: u64,
    backlog: Duration,
}

impl FixedStepClock {
    #[cfg(test)]
    pub(super) const fn new() -> Self {
        Self::after(TickId::new(0))
    }

    /// Resume logical time after a recovered durable system cursor. Scheduled
    /// work keeps its original tick; restart must never reuse a producer ID.
    pub(super) const fn after(last: TickId) -> Self {
        Self {
            last_tick: last.get(),
            backlog: Duration::ZERO,
        }
    }

    pub(super) fn advance(
        &mut self,
        elapsed: Duration,
        max_ticks: usize,
    ) -> Result<TickAdvance, ClockError> {
        self.backlog = self.backlog.saturating_add(elapsed);

        let available = self.backlog.as_nanos() / FIXED_STEP.as_nanos();
        let count = available.min(max_ticks as u128) as usize;
        let count_u64 = u64::try_from(count).map_err(|_| ClockError::TickIdExhausted)?;
        let end_tick = self
            .last_tick
            .checked_add(count_u64)
            .ok_or(ClockError::TickIdExhausted)?;

        let mut ticks = Vec::with_capacity(count);
        if count > 0 {
            for tick in (self.last_tick + 1)..=end_tick {
                ticks.push(TickId::new(tick));
                self.backlog -= FIXED_STEP;
            }
        }
        self.last_tick = end_tick;

        Ok(TickAdvance {
            ticks,
            backlog: self.backlog,
            backlog_ticks: (self.backlog.as_nanos() / FIXED_STEP.as_nanos()).min(u64::MAX as u128)
                as u64,
        })
    }

    #[cfg(test)]
    pub(super) const fn last_tick(&self) -> TickId {
        TickId::new(self.last_tick)
    }

    pub(super) const fn backlog(&self) -> Duration {
        self.backlog
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum QueueError<T> {
    Full {
        key: OrderKey,
        payload: T,
        capacity: usize,
    },
    DuplicateKey {
        key: OrderKey,
        payload: T,
    },
    ClosedTick {
        key: OrderKey,
        payload: T,
        closed_through: TickId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DrainError {
    TickAlreadyClosed {
        requested: TickId,
        closed_through: TickId,
    },
    UndrainedEarlierTick {
        requested: TickId,
        pending: TickId,
    },
}

/// A command after decoding, awaiting authoritative validation at input phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OrderedCommand<T> {
    pub(super) key: OrderKey,
    pub(super) payload: T,
}

/// Bounded, nonblocking command staging. Overflow and duplicate ordering keys
/// are returned to the caller for a visible reject/backpressure policy.
pub(super) struct CommandQueue<T> {
    capacity: usize,
    pending: Vec<OrderedCommand<T>>,
    keys: HashSet<OrderKey>,
    closed_through: Option<TickId>,
}

impl<T> CommandQueue<T> {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pending: Vec::with_capacity(capacity),
            keys: HashSet::with_capacity(capacity),
            closed_through: None,
        }
    }

    pub(super) fn try_push(&mut self, key: OrderKey, payload: T) -> Result<(), QueueError<T>> {
        if let Some(closed_through) = self.closed_through
            && key.tick() <= closed_through
        {
            return Err(QueueError::ClosedTick {
                key,
                payload,
                closed_through,
            });
        }
        if self.keys.contains(&key) {
            return Err(QueueError::DuplicateKey { key, payload });
        }
        if self.pending.len() == self.capacity {
            return Err(QueueError::Full {
                key,
                payload,
                capacity: self.capacity,
            });
        }

        self.keys.insert(key);
        self.pending.push(OrderedCommand { key, payload });
        Ok(())
    }

    /// Closes a tick and returns its commands sorted by `(tick, source, seq)`.
    /// Skipping a tick with queued commands is reported instead of silently
    /// leaving those commands stranded in the bounded queue.
    pub(super) fn drain_tick(
        &mut self,
        tick: TickId,
    ) -> Result<Vec<OrderedCommand<T>>, DrainError> {
        if let Some(closed_through) = self.closed_through
            && tick <= closed_through
        {
            return Err(DrainError::TickAlreadyClosed {
                requested: tick,
                closed_through,
            });
        }
        if let Some(earlier) = self
            .pending
            .iter()
            .map(|command| command.key.tick())
            .filter(|pending_tick| *pending_tick < tick)
            .min()
        {
            return Err(DrainError::UndrainedEarlierTick {
                requested: tick,
                pending: earlier,
            });
        }

        let mut ready = Vec::new();
        let mut future = Vec::with_capacity(self.pending.len());
        for command in self.pending.drain(..) {
            if command.key.tick() == tick {
                self.keys.remove(&command.key);
                ready.push(command);
            } else {
                future.push(command);
            }
        }
        self.pending = future;
        ready.sort_unstable_by_key(|command| command.key);
        self.closed_through = Some(tick);
        Ok(ready)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(super) const fn capacity(&self) -> usize {
        self.capacity
    }
}
