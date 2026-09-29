//! UI-independent replica presentation. One immutable downloaded module, one
//! outstanding worker call, and one replacement snapshot per connection.
use super::{Command, EffectBuffer, EntityView, Reply, Request, Script, Worker};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
#[cfg(test)]
#[path = "visual/tests.rs"]
mod tests;

struct PendingBatch {
    anchored: bool,
    entities: Vec<EntityView>,
    total: usize,
}

pub(crate) struct VisualSession {
    script: Arc<Script>,
    worker: Worker,
    sequence: u32,
    pending: Option<u32>,
    queued: VecDeque<PendingBatch>,
    poses: BTreeMap<u64, [f32; 3]>,
    tints: BTreeMap<u64, [f32; 3]>,
    previous_mobile: Vec<u64>,
    previous_anchors: Vec<u64>,
    anchor_positions: BTreeMap<u64, [f32; 3]>,
    effects: EffectBuffer,
    failure: Option<String>,
    parameters: crate::render::parameters::State,
}

impl VisualSession {
    #[cfg(test)]
    pub(crate) fn new(script: Arc<Script>) -> std::io::Result<Self> {
        Self::with_parameters(script, Default::default())
    }
    pub(crate) fn with_parameters(
        script: Arc<Script>,
        parameters: crate::render::parameters::State,
    ) -> std::io::Result<Self> {
        Ok(Self {
            script,
            worker: Worker::spawn()?,
            sequence: 0,
            pending: None,
            queued: VecDeque::new(),
            poses: BTreeMap::new(),
            tints: BTreeMap::new(),
            previous_mobile: Vec::new(),
            previous_anchors: Vec::new(),
            anchor_positions: BTreeMap::new(),
            effects: EffectBuffer::default(),
            failure: None,
            parameters,
        })
    }

    pub(crate) fn owner(&self) -> &str {
        self.script.module.split_once('@').map_or("", |v| v.0)
    }

    pub(crate) fn entities(&mut self, entities: Vec<EntityView>, total: usize) {
        self.queue(false, entities, total);
    }

    pub(crate) fn anchors(&mut self, entities: Vec<EntityView>, total: usize) {
        if total == 0
            && self.previous_anchors.is_empty()
            && !self.queued.iter().any(|batch| batch.anchored)
        {
            return;
        }
        self.queue(true, entities, total);
    }

    fn queue(&mut self, anchored: bool, entities: Vec<EntityView>, total: usize) {
        if self.failure.is_some() {
            return;
        }
        if entities.len() > 16
            || entities.windows(2).any(|pair| pair[0].id >= pair[1].id)
            || entities.iter().any(|entity| {
                entity.id == 0
                    || entity.key.len() > 129
                    || !entity.key.is_ascii()
                    || entity.position.iter().any(|axis| !axis.is_finite())
                    || entity.revision == 0
                    || (entity.motion_revision == 0) != anchored
                    || entity.public.len() > crate::protocol::MAX_PUBLIC_ENTITY_PAYLOAD
            })
        {
            self.failure = Some("invalid visual replica input".into());
            return;
        }
        if let Some(batch) = self
            .queued
            .iter_mut()
            .find(|batch| batch.anchored == anchored)
        {
            batch.entities = entities;
            batch.total = total;
        } else {
            self.queued.push_back(PendingBatch {
                anchored,
                entities,
                total,
            });
        }
        self.dispatch();
    }

    fn dispatch(&mut self) {
        if self.pending.is_some() || self.failure.is_some() {
            return;
        }
        let Some(batch) = self.queued.pop_front() else {
            return;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.failure = Some("visual sequence exhausted".into());
            return;
        };
        let previous = if batch.anchored {
            &self.previous_anchors
        } else {
            &self.previous_mobile
        };
        let (entered, left) = super::window_changes(previous, &batch.entities);
        let current = batch
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect::<Vec<_>>();
        let request = Request {
            script: Arc::clone(&self.script),
            sequence,
            event: if batch.anchored {
                "replica:anchors"
            } else {
                "replica:entities"
            }
            .into(),
            value: format!("total={}", batch.total),
            state: String::new(),
            texts: vec![],
            replica: true,
            entities: batch.entities,
            entered,
            left,
        };
        match self.worker.requests.try_send(request) {
            Ok(()) => {
                self.sequence = sequence;
                self.pending = Some(sequence);
                if batch.anchored {
                    self.previous_anchors = current;
                } else {
                    self.previous_mobile = current;
                }
            }
            Err(std::sync::mpsc::TrySendError::Full(request)) => {
                self.queued.push_front(PendingBatch {
                    anchored: batch.anchored,
                    entities: request.entities,
                    total: batch.total,
                });
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.failure = Some("visual worker stopped".into());
            }
        }
    }

    pub(crate) fn poll(&mut self) {
        if self.pending.is_some() {
            match self.worker.replies.try_recv() {
                Ok(reply) => self.apply(reply),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.failure = Some("visual worker stopped".into());
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        self.dispatch();
    }

    fn apply(&mut self, reply: Reply) {
        if self.pending != Some(reply.sequence) {
            return;
        }
        self.pending = None;
        let result = reply.result.and_then(|commands| {
            for command in &commands {
                if let Command::Parameter(update) = command {
                    self.parameters.check(self.owner(), update)?;
                }
            }
            if !reply.replica
                || !(reply.entity_batch || reply.anchor_batch)
                || commands.iter().any(|command| !match command {
                    Command::Visual(id, values) | Command::Tint(id, values) => {
                        reply.entity_batch
                            && reply.offered_entities.contains(id)
                            && values.iter().all(|value| value.is_finite())
                    }
                    Command::Ember(id, values) => {
                        reply.offered_entities.contains(id)
                            && values.iter().all(|value| value.is_finite())
                    }
                    Command::Spark(id, offset, color, size, lifetime_ms) => {
                        reply.offered_entities.contains(id)
                            && offset.iter().chain(color).all(|value| value.is_finite())
                            && (0.05..=0.5).contains(size)
                            && (100..=2000).contains(lifetime_ms)
                    }
                    Command::Parameter(_) => true,
                    _ => false,
                })
            {
                return Err("invalid visual replica command".into());
            }
            let mut poses = BTreeMap::new();
            let mut tints = BTreeMap::new();
            let mut embers = Vec::new();
            let mut sparks = Vec::new();
            for command in commands {
                match command {
                    Command::Visual(id, pose) => {
                        poses.insert(id, pose);
                    }
                    Command::Tint(id, tint) => {
                        tints.insert(id, tint);
                    }
                    Command::Ember(id, offset) => embers.push((id, offset)),
                    Command::Spark(id, offset, color, size, lifetime_ms) => {
                        sparks.push((id, offset, color, size, lifetime_ms))
                    }
                    Command::Parameter(update) => {
                        let owner = self.owner().to_owned();
                        self.parameters
                            .apply(&owner, &[update])
                            .expect("validated parameter batch");
                    }
                    _ => unreachable!("validated visual command"),
                }
            }
            Ok((poses, tints, embers, sparks))
        });
        match result {
            Ok((poses, tints, embers, sparks)) => {
                if reply.entity_batch {
                    self.poses = poses;
                    self.tints = tints;
                }
                if reply.anchor_batch {
                    self.anchor_positions = reply.offered_anchor_positions.into_iter().collect();
                }
                for (id, offset) in embers {
                    self.effects.push(id, offset);
                }
                for (id, offset, color, size, lifetime_ms) in sparks {
                    self.effects
                        .spark_with(id, offset, color, size, lifetime_ms);
                }
            }
            Err(error) => self.failure = Some(error),
        }
    }

    pub(crate) fn take_parameters(&mut self) -> Vec<crate::render::parameters::Update> {
        self.parameters.take_updates()
    }

    pub(crate) fn visual_pose(&self, id: u64) -> Option<[f32; 3]> {
        self.poses.get(&id).copied()
    }

    pub(crate) fn visual_tint(&self, id: u64) -> Option<[f32; 3]> {
        self.tints.get(&id).copied()
    }

    pub(crate) fn effects(
        &self,
        now: std::time::Instant,
        avatars: &[crate::render::VisualAvatar],
    ) -> Vec<crate::render::VisualFire> {
        self.effects.visuals(now, avatars, &self.anchor_positions)
    }

    #[cfg(test)]
    pub(crate) fn wait_for_test(&mut self) -> Result<(), String> {
        if self.pending.is_some() {
            let reply = self
                .worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(2))
                .map_err(|error| error.to_string())?;
            self.apply(reply);
        }
        self.failure.clone().map_or(Ok(()), Err)
    }
}
