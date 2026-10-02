//! Captured daylight phase and authorized, transaction-local clock control.
use super::{Context, Error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldTime {
    pub elapsed_ms: u64,
    pub cycle_ms: u64,
}

impl Context<'_> {
    /// Authenticated admin weather control staged in the complete transaction.
    pub fn admin_set_weather(&mut self, kind: u8, transition_ms: u32) -> Result<(), Error> {
        self.charge()?;
        if !self.snapshot.admin() {
            return self.fail(Error::Invalid("admin access denied".into()));
        }
        // 0 clear, 1 rain, 2 normal storm, 3 mild storm, 4 severe storm.
        if kind > 4 || transition_ms > 60_000 {
            return self.fail(Error::Invalid("invalid weather request".into()));
        }
        self.plan.weather = Some((kind, transition_ms));
        Ok(())
    }

    pub fn world_time(&mut self) -> Result<WorldTime, Error> {
        self.charge()?;
        let mut time = match self.snapshot.world_time() {
            Ok(time) => time,
            Err(error) => return self.fail(error),
        };
        if let Some(elapsed) = self.plan.world_time {
            time.elapsed_ms = elapsed;
        }
        Ok(time)
    }

    /// Shares the complete gameplay transaction. Only an authenticated admin
    /// actor may change the clock; a callback cannot create its own authority.
    pub fn admin_set_time(&mut self, elapsed_ms: u64) -> Result<(), Error> {
        self.charge()?;
        if !self.snapshot.admin() {
            return self.fail(Error::Invalid("admin access denied".into()));
        }
        let time = self.world_time()?;
        if elapsed_ms >= time.cycle_ms {
            return self.fail(Error::Invalid("invalid world time".into()));
        }
        self.plan.world_time = Some(elapsed_ms);
        Ok(())
    }
}
