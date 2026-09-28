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
            || (self.pending.is_none() && self.failure.is_none() && self.worker.is_some())
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
            let valid = commands.iter().all(|c| match c {
                Command::State(_) => true,
                Command::Text(id, _) => self.document().nodes.iter().any(|n| {
                    n.id == *id && matches!(n.kind, Kind::Label | Kind::Button | Kind::Input)
                }),
                Command::Visible(id, _) => self.document().nodes.iter().any(|n| n.id == *id),
            });
            if !valid {
                return Err(format!(
                    "{}: invalid local-ui target",
                    self.document().script.as_ref().unwrap().module
                ));
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
        } else if self.pending.is_some() {
            "BUSY: INPUT PAUSED"
        } else {
            "LOCAL UI"
        }
    }
}
