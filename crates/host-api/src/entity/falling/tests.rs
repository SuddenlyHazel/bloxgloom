use super::*;
use std::cell::Cell;

struct Column {
    reads: Cell<usize>,
    unavailable: bool,
}
impl FallingWorld for Column {
    fn solid(&self, _: [i32; 3]) -> Result<bool, Error> {
        self.reads.set(self.reads.get() + 1);
        if self.unavailable {
            Err(Error::OutOfRange)
        } else {
            Ok(false)
        }
    }
}

#[test]
fn quota_and_unavailable_reads_never_emit_partial_motion_and_retry_is_identical() {
    let column = Column {
        reads: Cell::new(0),
        unavailable: false,
    };
    let base = FallingContext {
        position: [0.5, 80.0, 0.5],
        vertical_speed: 0.0,
        suspended: false,
        tick: 10,
        step_seconds: 0.05,
        gravity: 24.0,
        terminal_speed: 30.0,
        radius: 0.18,
        world: &column,
    };
    assert_eq!(
        FallingContext {
            vertical_speed: -1000.0,
            step_seconds: 1.0,
            terminal_speed: 1000.0,
            ..base
        }
        .plan(),
        Err(Error::Exhausted)
    );
    assert_eq!(column.reads.get(), 0);
    let missing = Column {
        reads: Cell::new(0),
        unavailable: true,
    };
    assert_eq!(
        FallingContext {
            world: &missing,
            ..base
        }
        .plan(),
        Err(Error::OutOfRange)
    );
    assert_eq!(base.plan(), base.plan());
    assert_eq!(base.plan().unwrap().next_tick, Some(11));
}

#[test]
fn a_solid_corner_does_not_hide_a_missing_corner() {
    struct PartialColumn(Cell<usize>);
    impl FallingWorld for PartialColumn {
        fn solid(&self, _: [i32; 3]) -> Result<bool, Error> {
            if self.0.replace(self.0.get() + 1) == 0 {
                Ok(true)
            } else {
                Err(Error::OutOfRange)
            }
        }
    }
    let world = PartialColumn(Cell::new(0));
    let plan = FallingContext {
        position: [0.5, 80.0, 0.5],
        vertical_speed: 0.0,
        suspended: false,
        tick: 10,
        step_seconds: 0.05,
        gravity: 24.0,
        terminal_speed: 30.0,
        radius: 0.18,
        world: &world,
    };
    assert_eq!(plan.plan(), Err(Error::OutOfRange));
    assert_eq!(world.0.get(), 2);
}
