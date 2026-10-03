//! Serial immutable moderation lane; shutdown drops channels without joining Lua.
use bloxgloom_host_api::{
    chat::{Decision, Registration, Request, Route, valid_text},
    gameplay::Player,
};
use std::{
    collections::BTreeSet,
    sync::mpsc::{self, Receiver, SyncSender},
};
pub(super) struct Job {
    pub(super) request: Request,
    pub(super) hooks: Vec<Registration>,
}
type Audience = BTreeSet<(u128, u64)>;
type Output = std::result::Result<(String, Audience), String>;
pub(super) struct Result {
    pub(super) sender: Player,
    pub(super) output: Output,
}
pub(super) struct Lane {
    jobs: SyncSender<Job>,
    results: Receiver<Result>,
}
impl Lane {
    pub(super) fn spawn() -> std::io::Result<Self> {
        let (jobs, input) = mpsc::sync_channel::<Job>(16);
        let (output, results) = mpsc::sync_channel(16);
        std::thread::Builder::new()
            .name("chat-moderation".into())
            .spawn(move || {
                while let Ok(job) = input.recv() {
                    let sender = job.request.sender.clone();
                    let result = moderate(job);
                    if output
                        .send(Result {
                            sender,
                            output: result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self { jobs, results })
    }
    pub(super) fn enqueue(&self, job: Job) -> bool {
        self.jobs.try_send(job).is_ok()
    }
    pub(super) fn next(&self) -> Option<Result> {
        self.results.try_recv().ok()
    }
}
fn moderate(job: Job) -> Output {
    let mut request = job.request;
    let mut recipients = request
        .players
        .iter()
        .map(|p| (p.profile, p.session))
        .collect::<BTreeSet<_>>();
    for hook in job.hooks {
        match hook.moderator.moderate(&request) {
            Ok(Decision::Deny { reason }) if valid_text(&reason) => return Err(reason),
            Ok(Decision::Allow { text, route }) if valid_text(&text) => {
                if let Route::Sessions(targets) = route {
                    if targets.len() > bloxgloom_host_api::chat::MAX_RECIPIENTS
                        || targets.iter().any(|target| {
                            !request
                                .players
                                .iter()
                                .any(|p| (p.profile, p.session) == *target)
                        })
                    {
                        return Err("Chat hook selected a stale or invalid recipient.".into());
                    }
                    let targets = targets.into_iter().collect::<BTreeSet<_>>();
                    recipients.retain(|target| targets.contains(target));
                }
                request.text = text;
            }
            Ok(_) | Err(_) => return Err("Chat rejected by an invalid moderation decision.".into()),
        }
    }
    Ok((request.text, recipients))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Choose(Route);
    impl bloxgloom_host_api::chat::Moderator for Choose {
        fn moderate(&self, request: &Request) -> std::result::Result<Decision, String> {
            Ok(Decision::Allow {
                text: request.text.clone(),
                route: self.0.clone(),
            })
        }
    }
    fn player(profile: u128, session: u64) -> Player {
        Player {
            profile,
            session,
            entity: 0,
            name: "Rain".into(),
            position: [0.; 3],
            appearance: [0; 4],
            model: None,
            model_visual: None,
        }
    }
    #[test]
    fn moderators_can_only_narrow_to_captured_exact_sessions() {
        let request = Request {
            sender: player(1, 10),
            text: "hello".into(),
            players: vec![player(1, 10), player(2, 20)],
        };
        let hook = |route| Registration {
            key: "demo:chat".into(),
            revision: 1,
            moderator: std::sync::Arc::new(Choose(route)),
        };
        let output = moderate(Job {
            request: request.clone(),
            hooks: vec![hook(Route::Sessions(vec![(1, 10)])), hook(Route::All)],
        })
        .unwrap();
        assert_eq!(output.1, BTreeSet::from([(1, 10)]));
        assert!(
            moderate(Job {
                request: request.clone(),
                hooks: vec![hook(Route::Sessions(vec![(1, 9)]))]
            })
            .is_err()
        );
        assert!(
            moderate(Job {
                request,
                hooks: vec![hook(Route::Sessions(vec![(1, 10); 257]))]
            })
            .is_err()
        );
    }
}
