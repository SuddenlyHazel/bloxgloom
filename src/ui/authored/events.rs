//! Admission and atomic application of the closed local presentation protocol.
use super::*;
use crate::client::presentation::{Command, Reply, Request};

impl Session {
    pub(crate) fn activate(&mut self) {
        if let Some(i) = self
            .focused
            .filter(|&i| self.document().nodes[i].kind == Kind::Button && self.is_visible(i))
        {
            self.dispatch(i);
        }
    }

    pub(super) fn can_dispatch(&self, i: usize) -> bool {
        self.document().script.is_none()
            || self.document().nodes[i].event.is_none()
            || (self.pending.is_none()
                && self.action.is_none()
                && self.in_flight.is_none()
                && self.failure.is_none()
                && self.worker.is_some())
    }

    pub(super) fn dispatch(&mut self, i: usize) -> bool {
        let Some(script) = self.document().script.clone() else {
            return true;
        };
        let Some(event) = self.document().nodes[i].event.clone() else {
            return true;
        };
        if !self.can_dispatch(i) {
            return false;
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.failure = Some("presentation event sequence exhausted".into());
            return false;
        };
        let request = Request {
            script,
            sequence,
            event,
            value: self.inputs[i].clone(),
            state: self.state.clone(),
            texts: self
                .document()
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.kind, Kind::Label | Kind::Button | Kind::Input))
                .map(|(i, n)| (n.id.clone(), self.text_at(i).to_owned()))
                .collect(),
            replica: false,
            entities: vec![],
            entered: vec![],
            left: vec![],
        };
        if self
            .worker
            .as_ref()
            .unwrap()
            .requests
            .try_send(request)
            .is_err()
        {
            self.failure = Some("presentation worker unavailable".into());
            return false;
        }
        self.sequence = sequence;
        self.pending = Some(sequence);
        self.expected = Some(sequence);
        true
    }

    /// Nonblocking window-thread pump; at most one reply/application per frame.
    pub(crate) fn poll_presentation(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.replies.try_recv() {
            Ok(reply) => self.apply_reply(reply),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.failure = Some("presentation worker stopped".into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        self.dispatch_replica();
    }

    /// Replica notifications are advisory presentation inputs, not gameplay
    /// requests. Queue bounded snapshots while a local UI callback is running.
    pub(crate) fn replica_event(&mut self, event: &str, value: String) {
        self.queue_replica(event, value, vec![]);
    }

    pub(crate) fn replica_owner(&self) -> Option<&str> {
        self.startup
            .replica
            .as_ref()?
            .module
            .split_once('@')
            .map(|v| v.0)
    }

    pub(crate) fn replica_entities(
        &mut self,
        entities: Vec<crate::client::presentation::EntityView>,
        total: usize,
    ) {
        self.queue_replica("replica:entities", format!("total={total}"), entities);
    }

    pub(crate) fn visual_pose(&self, id: u64) -> Option<[f32; 3]> {
        self.visual_poses.get(&id).copied()
    }

    fn queue_replica(
        &mut self,
        event: &str,
        value: String,
        entities: Vec<crate::client::presentation::EntityView>,
    ) {
        if self.startup.replica.is_none() || self.failure.is_some() {
            return;
        }
        if event.len() > 64
            || value.len() > 640
            || !event.is_ascii()
            || !value.is_ascii()
            || entities.len() > 16
            || entities.windows(2).any(|pair| pair[0].id >= pair[1].id)
            || entities.iter().any(|entity| {
                entity.id == 0
                    || entity.key.len() > 129
                    || !entity.key.is_ascii()
                    || entity.position.iter().any(|axis| !axis.is_finite())
            })
        {
            self.failure = Some("invalid replica presentation input".into());
            return;
        }
        if let Some(existing) = self
            .replica_events
            .iter_mut()
            .find(|(kind, _, _)| kind == event)
        {
            existing.1 = value;
            existing.2 = entities;
        } else if self.replica_events.len() < 8 {
            self.replica_events
                .push_back((event.to_owned(), value, entities));
        } else {
            self.failure = Some("replica presentation queue exceeded".into());
            return;
        }
        self.dispatch_replica();
    }

    fn dispatch_replica(&mut self) {
        if self.pending.is_some() || self.failure.is_some() || self.replica_events.is_empty() {
            return;
        }
        let Some(script) = self.startup.replica.clone() else {
            return;
        };
        let Some(worker) = &self.worker else {
            self.failure = Some("presentation worker unavailable".into());
            return;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.failure = Some("presentation event sequence exhausted".into());
            return;
        };
        let (event, value, entities) = self.replica_events.front().unwrap();
        let (entered, left) = if event == "replica:entities" {
            crate::client::presentation::window_changes(&self.replica_previous, entities)
        } else {
            (vec![], vec![])
        };
        let current = entities.iter().map(|entity| entity.id).collect::<Vec<_>>();
        let request = Request {
            script,
            sequence,
            event: event.clone(),
            value: value.clone(),
            state: self.state.clone(),
            texts: self
                .document()
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| matches!(node.kind, Kind::Label | Kind::Button | Kind::Input))
                .map(|(index, node)| (node.id.clone(), self.text_at(index).to_owned()))
                .collect(),
            replica: true,
            entities: entities.clone(),
            entered,
            left,
        };
        match worker.requests.try_send(request) {
            Ok(()) => {
                if event == "replica:entities" {
                    self.replica_previous = current;
                }
                self.replica_events.pop_front();
                self.sequence = sequence;
                self.pending = Some(sequence);
                self.expected = Some(sequence);
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {}
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.failure = Some("presentation worker stopped".into())
            }
        }
    }

    /// Only offline previews/tests wait. Live dispatch always uses the pump.
    pub(crate) fn wait_for_presentation(&mut self) -> std::result::Result<(), String> {
        if self.pending.is_some() {
            let reply = self
                .worker
                .as_ref()
                .ok_or("missing presentation worker")?
                .replies
                .recv_timeout(std::time::Duration::from_secs(2))
                .map_err(|e| e.to_string())?;
            self.apply_reply(reply);
        }
        self.failure.clone().map_or(Ok(()), Err)
    }

    fn apply_reply(&mut self, reply: Reply) {
        if self.pending != Some(reply.sequence) {
            return;
        }
        self.pending = None;
        if self.expected.take() != Some(reply.sequence) {
            return;
        }
        let result = reply.result.and_then(|commands| {
            // Validate the whole batch before modifying anything. Target IDs must
            // name this exact document, not even another document of this package.
            let owner = self.document().id.split_once(':').unwrap().0;
            let valid = commands.iter().all(|c| match c {
                Command::State(_) => true,
                Command::Text(id, _) => {
                    (!reply.replica || self.replica_owner() == Some(owner))
                        && self.document().nodes.iter().any(|n| {
                            n.id == *id
                                && matches!(n.kind, Kind::Label | Kind::Button | Kind::Input)
                        })
                }
                Command::Visible(id, _) => {
                    (!reply.replica || self.replica_owner() == Some(owner))
                        && self.document().nodes.iter().any(|n| n.id == *id)
                }
                Command::Action(key) if !reply.replica => key
                    .split_once(':')
                    .is_some_and(|(package, local)| package == owner && identifier(local)),
                Command::Action(_) => false,
                Command::Visual(id, pose) => {
                    reply.replica
                        && reply.entity_batch
                        && reply.offered_entities.contains(id)
                        && pose.iter().all(|value| value.is_finite())
                }
            }) && commands
                .iter()
                .filter(|c| matches!(c, Command::Action(_)))
                .count()
                <= 1
                && (self.action.is_none() && self.in_flight.is_none()
                    || !commands.iter().any(|c| matches!(c, Command::Action(_))));
            if !valid {
                return Err(format!(
                    "{}: invalid local-ui target",
                    if reply.replica {
                        "replica handler"
                    } else {
                        &self.document().script.as_ref().unwrap().module
                    }
                ));
            }
            if reply.entity_batch {
                self.visual_poses.clear();
            }
            for command in commands {
                match command {
                    Command::State(value) => self.state = value,
                    Command::Text(id, value) => {
                        let i = self
                            .document()
                            .nodes
                            .iter()
                            .position(|n| n.id == id)
                            .unwrap();
                        if self.document().nodes[i].kind == Kind::Input {
                            self.inputs[i] = value;
                        } else {
                            self.texts[i] = value;
                        }
                    }
                    Command::Visible(id, visible) => {
                        let i = self
                            .document()
                            .nodes
                            .iter()
                            .position(|n| n.id == id)
                            .unwrap();
                        self.visible[i] = visible;
                    }
                    Command::Action(key) => {
                        self.feedback = Some("REQUESTING ACTION".into());
                        self.action = Some(key);
                    }
                    Command::Visual(id, pose) => {
                        self.visual_poses.insert(id, pose);
                    }
                }
            }
            if self.focused.is_some_and(|i| !self.is_visible(i)) {
                self.focused = None;
            }
            Ok(())
        });
        if let Err(error) = result {
            eprintln!("client presentation event {}: {error}", reply.sequence);
            self.failure = Some(error);
        }
    }

    pub(crate) fn text_at(&self, i: usize) -> &str {
        if self.document().nodes[i].kind == Kind::Input {
            &self.inputs[i]
        } else {
            &self.texts[i]
        }
    }

    pub(crate) fn take_action(&mut self) -> Option<String> {
        self.action.take()
    }

    pub(crate) fn action_submitted(&mut self, id: u128) {
        self.in_flight = Some((id, self.document_generation));
        self.feedback = Some("WAITING FOR SERVER".into());
    }

    pub(crate) fn action_failed_locally(&mut self, reason: &str) {
        self.feedback = Some(format!("ACTION NOT SENT: {reason}"));
    }

    #[cfg(test)]
    pub(crate) fn feedback(&self) -> Option<&str> {
        self.feedback.as_deref()
    }

    pub(crate) fn action_result(&mut self, id: u128, accepted: bool, reason: &str) {
        if let Some((pending, generation)) = self.in_flight
            && pending == id
        {
            self.in_flight = None;
            if generation == self.document_generation {
                self.feedback = Some(if accepted {
                    "SERVER APPLIED ACTION".into()
                } else {
                    format!("SERVER DENIED: {reason}")
                });
            }
        }
    }

    pub(super) fn is_visible(&self, mut i: usize) -> bool {
        loop {
            if !self.visible[i] {
                return false;
            }
            let Some(parent) = self.document().nodes[i].parent else {
                return true;
            };
            i = parent;
        }
    }

    pub(super) fn event_status(&self) -> &str {
        if self.document().script.is_none() {
            "UNBOUND"
        } else if self.failure.is_some() || self.worker.is_none() {
            "DISABLED: HANDLER ERROR"
        } else if self.pending.is_some() || self.action.is_some() || self.in_flight.is_some() {
            "BUSY: INPUT PAUSED"
        } else {
            "LOCAL UI"
        }
    }
}
