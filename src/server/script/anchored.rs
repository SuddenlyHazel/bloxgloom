//! Server-only pure own-state callbacks on the native anchored lifecycle.
use super::{
    Invocation, Limits, Program,
    package::PackageSnapshot,
    run_with,
    runtime::{Execution, Seed},
};
use bloxgloom_host_api::{
    anchored as api,
    entity::{Error, Payload},
};
use mlua::{Lua, Value};
use std::sync::Arc;
#[path = "anchored/input.rs"]
mod input;
#[path = "anchored/reply.rs"]
mod reply;
use input::Event;
use reply::Reply;

pub(in crate::server::script) struct ScriptAnchored {
    snapshot: Option<Arc<PackageSnapshot>>,
    module: String,
    state_limit: usize,
    public_limit: usize,
}
impl ScriptAnchored {
    pub(in crate::server::script) fn server(
        snapshot: Arc<PackageSnapshot>,
        module: String,
        state_limit: usize,
        public_limit: usize,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            module,
            state_limit,
            public_limit,
        }
    }
    pub(in crate::server::script) fn client() -> Self {
        Self {
            snapshot: None,
            module: String::new(),
            state_limit: 65536,
            public_limit: 4096,
        }
    }
    fn bytes<'a>(&self, state: &'a Payload) -> Result<&'a [u8], Error> {
        state
            .downcast_ref::<Vec<u8>>()
            .filter(|s| s.len() <= self.state_limit)
            .map(Vec::as_slice)
            .ok_or(Error::InvalidState)
    }
    fn call(&self, event: Event<'_>) -> Result<Reply, Error> {
        let snapshot = self.snapshot.as_ref().ok_or(Error::InvalidState)?;
        run_with(
            &Program::Package{snapshot:Arc::clone(snapshot),entry:self.module.clone(),invocation:Invocation::Integer},
            Limits::default(),
            Execution::new("anchored",event.seed(), format!("{}:{}",self.module,event.kind)),
            |lua,entry|{
                let value=entry.call::<Value>(event.table(lua)?)?;
                reply::parse(value,&event,self.state_limit,self.public_limit)
            },
        ).map_err(|error|{tracing::warn!(%error,module=%self.module,event=event.kind,"anchored callback rejected without effects");Error::InvalidState})
    }
    fn state(&self, event: Event<'_>) -> Result<Payload, Error> {
        let Reply::Bytes(bytes) = self.call(event)? else {
            return Err(Error::InvalidState);
        };
        Ok(Payload::new(bytes))
    }
}
impl api::Behavior for ScriptAnchored {
    fn initialize(&self, anchor: [i32; 3]) -> Result<Payload, Error> {
        let mut event = Event::new("Initialize", &[]);
        event.anchor = Some(anchor);
        self.state(event)
    }
    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error> {
        if bytes.len() > self.state_limit {
            return Err(Error::InvalidState);
        }
        if self.snapshot.is_some()
            && !matches!(self.call(Event::new("Validate", bytes))?, Reply::Valid)
        {
            return Err(Error::InvalidState);
        }
        Ok(Payload::new(bytes.to_vec()))
    }
    fn encode(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        Ok(self.bytes(state)?.to_vec())
    }
    fn public(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        let Reply::Bytes(bytes) = self.call(Event::new("Public", self.bytes(state)?))? else {
            return Err(Error::InvalidState);
        };
        Ok(bytes)
    }
    fn react(&self, context: &api::Context<'_>) -> Result<api::Reaction, Error> {
        let mut event = Event::new("React", self.bytes(context.state)?);
        event.anchor = Some(context.anchor);
        event.tick = Some(context.tick);
        event.cells = context.cells;
        let Reply::Reaction(reaction) = self.call(event)? else {
            return Err(Error::InvalidState);
        };
        Ok(reaction)
    }
    fn interact(&self, state: &Payload, request: &[u8]) -> Result<Payload, Error> {
        let mut event = Event::new("Interact", self.bytes(state)?);
        event.request = Some(request);
        self.state(event)
    }
    fn refund(
        &self,
        state: &Payload,
        cause: api::RemovalCause,
        maximum: u16,
    ) -> Result<u16, Error> {
        let mut event = Event::new("Refund", self.bytes(state)?);
        event.cause = Some(match cause {
            api::RemovalCause::Broken => "Broken",
            api::RemovalCause::Reaction => "Reaction",
            api::RemovalCause::WorldEdit => "WorldEdit",
        });
        event.maximum = Some(maximum);
        let Reply::Refund(count) = self.call(event)? else {
            return Err(Error::InvalidState);
        };
        Ok(count)
    }
}
