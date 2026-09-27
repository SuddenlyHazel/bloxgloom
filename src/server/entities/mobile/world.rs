use super::*;
use std::cell::{Cell, RefCell};
pub(super) struct World<'a> {
    view: &'a VoxelView,
    body: super::super::locomotion::Body,
    position: [f32; 3],
    pub failed: Cell<bool>,
    pub movements: RefCell<Vec<[f32; 3]>>,
}
impl<'a> World<'a> {
    pub fn new(view: &'a VoxelView, body: api::Body, position: [f32; 3]) -> Self {
        Self {
            view,
            body: super::super::locomotion::Body {
                half_width: body.half_width,
                height: body.height,
                speed: body.speed,
            },
            position,
            failed: Cell::new(false),
            movements: RefCell::new(vec![]),
        }
    }
    fn result<T>(&self, value: Result<T, EntityError>) -> Result<T, api::Error> {
        value.map_err(|e| {
            if e == EntityError::ViewOutOfRange {
                self.failed.set(true);
                api::Error::OutOfRange
            } else {
                api::Error::InvalidState
            }
        })
    }
    fn position(&self, p: [f32; 3]) -> Result<(), api::Error> {
        if !super::super::locomotion::valid_position(p)
            || (0..3).any(|i| (p[i] - self.position[i]).abs() > 16.0)
        {
            self.failed.set(true);
            return Err(api::Error::OutOfRange);
        }
        Ok(())
    }
}
impl api::World for World<'_> {
    fn solid(&self, p: [i32; 3]) -> Result<bool, api::Error> {
        self.result(
            self.view
                .is_solid(p[0], p[1], p[2])
                .map_err(|_| EntityError::ViewOutOfRange),
        )
    }
    fn clear(&self, p: [f32; 3]) -> Result<bool, api::Error> {
        self.position(p)?;
        self.result(self.body.clear(self.view, p))
    }
    fn grounded(&self, p: [f32; 3]) -> Result<bool, api::Error> {
        self.position(p)?;
        self.result(self.body.grounded(self.view, p))
    }
    fn walk_edge(&self, from: [f32; 3], to: [f32; 3]) -> Result<bool, api::Error> {
        self.position(from)?;
        self.position(to)?;
        self.result(self.body.walk_edge(self.view, from, to))
    }
    fn route(&self, p: [f32; 3], goal: [i32; 2]) -> Result<api::Route, api::Error> {
        self.position(p)?;
        self.result(super::super::navigation::route(
            self.view, self.body, p, goal,
        ))
        .map(|r| match r {
            super::super::navigation::Route::Arrived => api::Route::Arrived,
            super::super::navigation::Route::Next(p) => api::Route::Next(p),
            super::super::navigation::Route::Unreachable => api::Route::Unreachable,
            super::super::navigation::Route::BudgetExhausted => api::Route::BudgetExhausted,
        })
    }
    fn advance(
        &self,
        p: [f32; 3],
        velocity: f32,
        target: Option<[f32; 3]>,
    ) -> Result<api::Movement, api::Error> {
        if p != self.position
            || !velocity.is_finite()
            || !(-24.0..=0.0).contains(&velocity)
            || self.movements.borrow().len() >= 4
        {
            return Err(api::Error::InvalidState);
        }
        if let Some(target) = target {
            self.position(target)?;
        }
        let m = self.result(self.body.advance(self.view, p, velocity, target))?;
        self.movements.borrow_mut().push(m.position);
        Ok(api::Movement {
            position: m.position,
            vertical_velocity: m.vertical_velocity,
            grounded: m.grounded,
        })
    }
}
