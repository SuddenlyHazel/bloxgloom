//! Independent custom codec, initialization, use and level-triggered support.
use bloxgloom_host_api::{
    anchored::*,
    entity::{Error, Payload},
    *,
};
use std::sync::Arc;
pub const KEY: &str = "fixture:signal_post";
pub struct SignalPost;
#[derive(Clone)]
struct State {
    active: bool,
    neighbor: bool,
    height: i32,
}
impl Behavior for SignalPost {
    fn initialize(&self, anchor: [i32; 3]) -> Result<Payload, Error> {
        Ok(Payload::new(State {
            active: false,
            neighbor: false,
            height: anchor[1],
        }))
    }
    fn decode(&self, b: &[u8]) -> Result<Payload, Error> {
        if b.len() != 6 || b[0] > 1 || b[1] > 1 {
            return Err(Error::InvalidState);
        }
        Ok(Payload::new(State {
            active: b[0] != 0,
            neighbor: b[1] != 0,
            height: i32::from_le_bytes(b[2..].try_into().unwrap()),
        }))
    }
    fn encode(&self, p: &Payload) -> Result<Vec<u8>, Error> {
        let s = p.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        let mut b = vec![u8::from(s.active), u8::from(s.neighbor)];
        b.extend(s.height.to_le_bytes());
        Ok(b)
    }
    fn public(&self, p: &Payload) -> Result<Vec<u8>, Error> {
        Ok(self.encode(p)?[..2].to_vec())
    }
    fn react(&self, c: &Context<'_>) -> Result<Reaction, Error> {
        if !c
            .cells
            .iter()
            .find(|c| c.offset == [0, -1, 0])
            .ok_or(Error::OutOfRange)?
            .solid
        {
            return Ok(Reaction::Remove);
        }
        let mut s = c
            .state
            .downcast_ref::<State>()
            .ok_or(Error::InvalidState)?
            .clone();
        let neighbor = c
            .cells
            .iter()
            .find(|c| c.offset == [1, 0, 0])
            .ok_or(Error::OutOfRange)?
            .solid;
        if neighbor == s.neighbor {
            return Ok(Reaction::Keep);
        }
        s.neighbor = neighbor;
        Ok(Reaction::Update(Payload::new(s)))
    }
    fn interact(&self, p: &Payload, request: &[u8]) -> Result<Payload, Error> {
        if request != b"toggle" {
            return Err(Error::InvalidState);
        }
        let mut s = p
            .downcast_ref::<State>()
            .ok_or(Error::InvalidState)?
            .clone();
        s.active = !s.active;
        Ok(Payload::new(s))
    }
}
impl Extension for SignalPost {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        r.cube_block(CubeBlock {
            key: KEY.into(),
            name: "SIGNAL POST".into(),
            texture: "bloxgloom:chest_side".into(),
        })?;
        r.anchored_block_entity(AnchoredBlockEntity {
            entity: KEY.into(),
            block: KEY.into(),
            placement_item: KEY.into(),
            anchor_state: KEY.into(),
            footprint: vec![
                FootprintCell {
                    offset: [0, 0, 0],
                    state: KEY.into(),
                },
                FootprintCell {
                    offset: [0, 1, 0],
                    state: KEY.into(),
                },
            ],
            placement_cost: 3,
            removal_refund: 2,
            schema_version: 1,
            schema_fingerprint: 0x5101,
            max_state_bytes: 6,
            max_public_bytes: 2,
            interval: 20,
            observe: vec![[0, -1, 0], [1, 0, 0]],
            interaction: b"toggle".to_vec(),
            behavior: Arc::new(Self),
        })
    }
}
