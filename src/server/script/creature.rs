//! Server-only Luau planning for authored mobile creatures. The same bounded
//! codec supplies inert client pose reconstruction; client catalogs never run
//! the server callback or receive private creature state.
use super::{Invocation, Limits, Program, package::PackageSnapshot, run_with};
use bloxgloom_host_api::entity::{self as api, Behavior, Error, Lifecycle, Payload};
use mlua::{Function, Lua, Value};
use std::sync::Arc;

mod services;
use services::invoke;

#[derive(Clone)]
struct State {
    yaw: f32,
    velocity: f32,
    grounded: bool,
    private: Vec<u8>,
}

pub(in crate::server::script) struct ScriptCreature {
    snapshot: Option<Arc<PackageSnapshot>>,
    key: String,
    module: String,
    initial: Vec<u8>,
    max_private: usize,
}

impl ScriptCreature {
    pub(in crate::server::script) fn server(
        snapshot: Arc<PackageSnapshot>,
        key: String,
        module: String,
        initial: Vec<u8>,
        max_private: usize,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            key,
            module,
            initial,
            max_private,
        }
    }

    pub(in crate::server::script) fn client(max_private: usize) -> Self {
        Self {
            snapshot: None,
            key: String::new(),
            module: String::new(),
            initial: Vec::new(),
            max_private,
        }
    }

    fn valid(&self, state: &State) -> bool {
        state.yaw.is_finite()
            && state.velocity.is_finite()
            && (-24.0..=0.0).contains(&state.velocity)
            && state.private.len() <= self.max_private
    }
}

impl Behavior for ScriptCreature {
    fn initial(&self) -> Payload {
        Payload::new(State {
            yaw: 0.0,
            velocity: 0.0,
            grounded: false,
            private: self.initial.clone(),
        })
    }

    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error> {
        if bytes.len() < 12 || bytes[0] != 1 || bytes[9] > 1 {
            return Err(Error::InvalidState);
        }
        let len = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
        if bytes.len() != 12 + len {
            return Err(Error::InvalidState);
        }
        let state = State {
            yaw: f32::from_le_bytes(bytes[1..5].try_into().unwrap()),
            velocity: f32::from_le_bytes(bytes[5..9].try_into().unwrap()),
            grounded: bytes[9] == 1,
            private: bytes[12..].to_vec(),
        };
        self.valid(&state)
            .then(|| Payload::new(state))
            .ok_or(Error::InvalidState)
    }

    fn encode(&self, payload: &Payload) -> Result<Vec<u8>, Error> {
        let state = payload.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        if !self.valid(state) {
            return Err(Error::InvalidState);
        }
        let mut bytes = Vec::with_capacity(12 + state.private.len());
        bytes.push(1);
        bytes.extend(state.yaw.to_le_bytes());
        bytes.extend(state.velocity.to_le_bytes());
        bytes.push(u8::from(state.grounded));
        bytes.extend((state.private.len() as u16).to_le_bytes());
        bytes.extend(&state.private);
        Ok(bytes)
    }

    fn public(&self, payload: &Payload) -> Result<Vec<u8>, Error> {
        let state = payload.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        if !self.valid(state) {
            return Err(Error::InvalidState);
        }
        let mut bytes = state.yaw.to_le_bytes().to_vec();
        bytes.push(u8::from(state.grounded));
        Ok(bytes)
    }

    fn pose(&self, public: &[u8]) -> Result<api::Pose, Error> {
        if public.len() != 5 || public[4] > 1 {
            return Err(Error::InvalidState);
        }
        let yaw = f32::from_le_bytes(public[..4].try_into().unwrap());
        if !yaw.is_finite() {
            return Err(Error::InvalidState);
        }
        Ok(api::Pose {
            yaw,
            grounded: public[4] == 1,
        })
    }

    fn tick(&self, context: &api::Context<'_>) -> Result<api::Plan, Error> {
        let mut plan = api::Plan {
            state: None,
            next_tick: context.next_tick,
            position: None,
            lifecycle: Lifecycle::default(),
        };
        if context.next_tick.is_some_and(|due| due > context.tick) {
            return Ok(plan);
        }
        let Some(snapshot) = &self.snapshot else {
            return Err(Error::InvalidState);
        };
        let mut state = context
            .state
            .downcast_ref::<State>()
            .ok_or(Error::InvalidState)?
            .clone();
        let result = run_with(
            &Program::Package {
                snapshot: Arc::clone(snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            |lua, entry| invoke(lua, entry, context, &state.private, self.max_private),
        );
        let result = result.map_err(|error| {
            eprintln!("creature {} tick rejected: {error}", self.module);
            Error::InvalidState
        })?;
        let owner = self.key.split_once(':').ok_or(Error::InvalidState)?.0;
        let spawns = result
            .lifecycle
            .spawns
            .into_iter()
            .map(|request| {
                let key = request.key.unwrap_or_else(|| self.key.clone());
                if key.split_once(':').map(|parts| parts.0) != Some(owner) {
                    return Err(Error::InvalidState);
                }
                let state = snapshot.creature_initial(&key).ok_or(Error::InvalidState)?;
                Ok(api::Spawn {
                    key,
                    position: request.position,
                    state,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let movement = context
            .world
            .advance(context.position, state.velocity, result.target)?;
        state.private = result.private;
        state.velocity = movement.vertical_velocity;
        state.grounded = movement.grounded;
        let dx = movement.position[0] - context.position[0];
        let dz = movement.position[2] - context.position[2];
        if dx.abs() + dz.abs() > 0.0001 {
            state.yaw = dx.atan2(dz);
        }
        if !self.valid(&state) {
            return Err(Error::InvalidState);
        }
        plan.state = Some(Payload::new(state));
        plan.lifecycle.despawn = result.lifecycle.despawn;
        plan.lifecycle.spawns = spawns;
        plan.position = (movement.position != context.position).then_some(movement.position);
        plan.next_tick = Some(
            context
                .tick
                .checked_add(u64::from(result.delay))
                .ok_or(Error::Exhausted)?,
        );
        Ok(plan)
    }

    fn interact(&self, payload: &Payload, request: &[u8]) -> Result<Payload, Error> {
        let Some(snapshot) = &self.snapshot else {
            return Err(Error::InvalidState);
        };
        let mut state = payload
            .downcast_ref::<State>()
            .ok_or(Error::InvalidState)?
            .clone();
        let private = run_with(
            &Program::Package {
                snapshot: Arc::clone(snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            |lua, entry| {
                let host = lua.create_table()?;
                host.set("event", "interact")?;
                host.set("data", lua.create_string(&state.private)?)?;
                host.set("request", lua.create_string(request)?)?;
                host.set_readonly(true);
                let result: Value = entry.call(host)?;
                let Value::String(result) = result else {
                    return Err(invalid("creature interaction must return binary state"));
                };
                if result.as_bytes().len() > self.max_private {
                    return Err(invalid("creature interaction state exceeds bound"));
                }
                Ok(result.as_bytes().to_vec())
            },
        )
        .map_err(|error| {
            eprintln!("creature {} interaction rejected: {error}", self.module);
            Error::InvalidState
        })?;
        state.private = private;
        Ok(Payload::new(state))
    }
}

fn invalid(message: &'static str) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}
