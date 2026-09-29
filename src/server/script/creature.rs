//! Server-only Luau planning for authored mobile creatures. The same bounded
//! codec supplies inert client pose reconstruction; client catalogs never run
//! the server callback or receive private creature state.
use super::{Invocation, Limits, Program, package::PackageSnapshot, run_with, values::integer};
use bloxgloom_host_api::entity::{self as api, Behavior, Error, Lifecycle, Payload};
use mlua::{Function, Lua, Value};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

#[derive(Clone)]
struct State {
    yaw: f32,
    velocity: f32,
    grounded: bool,
    private: Vec<u8>,
}

pub(in crate::server::script) struct ScriptCreature {
    snapshot: Option<Arc<PackageSnapshot>>,
    module: String,
    initial: Vec<u8>,
    max_private: usize,
}

impl ScriptCreature {
    pub(in crate::server::script) fn server(
        snapshot: Arc<PackageSnapshot>,
        module: String,
        initial: Vec<u8>,
        max_private: usize,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            module,
            initial,
            max_private,
        }
    }

    pub(in crate::server::script) fn client(max_private: usize) -> Self {
        Self {
            snapshot: None,
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
        let (private, delay, target) = result.map_err(|error| {
            eprintln!("creature {} tick rejected: {error}", self.module);
            Error::InvalidState
        })?;
        let movement = context
            .world
            .advance(context.position, state.velocity, target)?;
        state.private = private;
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
        plan.position = (movement.position != context.position).then_some(movement.position);
        plan.next_tick = Some(
            context
                .tick
                .checked_add(u64::from(delay))
                .ok_or(Error::Exhausted)?,
        );
        Ok(plan)
    }
}

fn invoke(
    lua: &Lua,
    entry: Function,
    context: &api::Context<'_>,
    private: &[u8],
    max_private: usize,
) -> mlua::Result<(Vec<u8>, u32, Option<[f32; 3]>)> {
    let host = lua.create_table()?;
    host.set("data", lua.create_string(private)?)?;
    host.set("id_lo", context.id as u32)?;
    host.set("id_hi", (context.id >> 32) as u32)?;
    host.set("tick_lo", context.tick as u32)?;
    host.set("tick_hi", (context.tick >> 32) as u32)?;
    let position = lua.create_sequence_from(context.position)?;
    position.set_readonly(true);
    host.set("position", position)?;
    let routes = Cell::new(0u8);
    let rejected = RefCell::new(None);
    lua.scope(|scope| {
        host.set(
            "route",
            scope.create_function(|lua, (x, z): (Value, Value)| {
                let value = (|| {
                    if let Some(error) = *rejected.borrow() {
                        return Err(invalid(error));
                    }
                    if routes.get() >= 8 {
                        return Err(invalid("creature route limit exceeded"));
                    }
                    routes.set(routes.get() + 1);
                    let x = integer(x, -1_000_000, 1_000_000).map_err(invalid)? as i32;
                    let z = integer(z, -1_000_000, 1_000_000).map_err(invalid)? as i32;
                    match context
                        .world
                        .route(context.position, [x, z])
                        .map_err(|_| invalid("creature route unavailable"))?
                    {
                        api::Route::Next(point) => {
                            let result = lua.create_sequence_from(point)?;
                            result.set_readonly(true);
                            Ok(Value::Table(result))
                        }
                        api::Route::Arrived | api::Route::Unreachable => Ok(Value::Nil),
                        api::Route::BudgetExhausted => {
                            Err(invalid("creature route budget exceeded"))
                        }
                    }
                })();
                if value.is_err() {
                    rejected.borrow_mut().get_or_insert("creature route failed");
                }
                value
            })?,
        )?;
        host.set_readonly(true);
        let (data, delay, x, z): (Value, Value, Value, Value) = entry.call(host)?;
        if let Some(error) = *rejected.borrow() {
            return Err(invalid(error));
        }
        let Value::String(data) = data else {
            return Err(invalid("creature state must be binary string"));
        };
        if data.as_bytes().len() > max_private {
            return Err(invalid("creature state exceeds bound"));
        }
        let delay = integer(delay, 1, 100_000).map_err(invalid)? as u32;
        let target = if x.is_nil() && z.is_nil() {
            None
        } else if !x.is_nil() && !z.is_nil() {
            Some([coordinate(x)?, context.position[1], coordinate(z)?])
        } else {
            return Err(invalid("creature target needs x and z"));
        };
        Ok((data.as_bytes().to_vec(), delay, target))
    })
}

fn coordinate(value: Value) -> mlua::Result<f32> {
    let number = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err(invalid("creature target must be numeric")),
    };
    if !number.is_finite() || !(-1_000_000.0..=1_000_000.0).contains(&number) {
        return Err(invalid("creature target out of bounds"));
    }
    Ok(number as f32)
}

fn invalid(message: &'static str) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}
