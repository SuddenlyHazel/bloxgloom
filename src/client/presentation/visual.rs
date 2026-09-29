//! UI-independent replica presentation. One immutable downloaded module, one
//! outstanding worker call, and one replacement snapshot per connection.
use super::{Command, EffectBuffer, EntityView, Reply, Request, Script, Worker};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) struct VisualSession {
    script: Arc<Script>,
    worker: Worker,
    sequence: u32,
    pending: Option<u32>,
    queued: Option<(Vec<EntityView>, usize)>,
    poses: BTreeMap<u64, [f32; 3]>,
    tints: BTreeMap<u64, [f32; 3]>,
    previous: Vec<u64>,
    effects: EffectBuffer,
    failure: Option<String>,
}

impl VisualSession {
    pub(crate) fn new(script: Arc<Script>) -> std::io::Result<Self> {
        Ok(Self {
            script,
            worker: Worker::spawn()?,
            sequence: 0,
            pending: None,
            queued: None,
            poses: BTreeMap::new(),
            tints: BTreeMap::new(),
            previous: Vec::new(),
            effects: EffectBuffer::default(),
            failure: None,
        })
    }

    pub(crate) fn owner(&self) -> &str {
        self.script.module.split_once('@').map_or("", |v| v.0)
    }

    pub(crate) fn entities(&mut self, entities: Vec<EntityView>, total: usize) {
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
                    || entity.motion_revision == 0
                    || entity.public.len() > crate::protocol::MAX_PUBLIC_ENTITY_PAYLOAD
            })
        {
            self.failure = Some("invalid visual replica input".into());
            return;
        }
        self.queued = Some((entities, total));
        self.dispatch();
    }

    fn dispatch(&mut self) {
        if self.pending.is_some() || self.failure.is_some() {
            return;
        }
        let Some((entities, total)) = self.queued.take() else {
            return;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.failure = Some("visual sequence exhausted".into());
            return;
        };
        let (entered, left) = super::window_changes(&self.previous, &entities);
        let current = entities.iter().map(|entity| entity.id).collect::<Vec<_>>();
        let request = Request {
            script: Arc::clone(&self.script),
            sequence,
            event: "replica:entities".into(),
            value: format!("total={total}"),
            state: String::new(),
            texts: vec![],
            replica: true,
            entities,
            entered,
            left,
        };
        match self.worker.requests.try_send(request) {
            Ok(()) => {
                self.sequence = sequence;
                self.pending = Some(sequence);
                self.previous = current;
            }
            Err(std::sync::mpsc::TrySendError::Full(request)) => {
                self.queued = Some((request.entities, total));
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
            if !reply.replica
                || !reply.entity_batch
                || commands.iter().any(|command| {
                    !matches!(command,
                        Command::Visual(id, values) | Command::Tint(id, values) | Command::Ember(id, values)
                        if reply.offered_entities.contains(id) && values.iter().all(|v| v.is_finite()))
                })
            {
                return Err("invalid visual replica command".into());
            }
            let mut poses = BTreeMap::new();
            let mut tints = BTreeMap::new();
            let mut embers = Vec::new();
            for command in commands {
                match command {
                    Command::Visual(id, pose) => {
                        poses.insert(id, pose);
                    }
                    Command::Tint(id, tint) => {
                        tints.insert(id, tint);
                    }
                    Command::Ember(id, offset) => embers.push((id, offset)),
                    _ => unreachable!("validated visual command"),
                }
            }
            Ok((poses, tints, embers))
        });
        match result {
            Ok((poses, tints, embers)) => {
                self.poses = poses;
                self.tints = tints;
                for (id, offset) in embers {
                    self.effects.push(id, offset);
                }
            }
            Err(error) => self.failure = Some(error),
        }
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
        self.effects.visuals(now, avatars)
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
