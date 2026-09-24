//! Headless load benchmark for the authoritative server tick path.
//!
//! Setup (world generation, client registration, and drop seeding) is timed
//! separately. Measured ticks call the same `tick_with_inputs` coordinator
//! function as the live server and are paced at the production 50 Hz rate.

mod fixture;
mod report;

#[cfg(test)]
#[path = "perf/tests.rs"]
mod tests;

use super::metrics::MetricsRecorder;
use super::server_state;
use super::simulation::{FIXED_STEP, TickId};
#[cfg(test)]
use crate::world::world_to_chunk;
use fixture::{
    ACTION_INTERVAL, DrainTotals, MAX_STEADY_TICKS, MIN_STEADY_TICKS, PLAYER_COUNT, SEED, Scenario,
    TempSaveDir, drain_outbound, prepare, tick_inputs,
};
#[cfg(test)]
use std::collections::HashSet;
use std::io;
use std::thread;
use std::time::Instant;

/// Runs two isolated 16-player scenarios: a dense cluster and a spatially
/// spread group. At least 300 paced 50 Hz ticks are required so short startup
/// effects do not dominate the reported steady-state percentiles.
pub fn run_perf_benchmark(steady_ticks: usize) -> io::Result<()> {
    validate_tick_count(steady_ticks)?;
    println!(
        "authoritative server benchmark: {steady_ticks} steady ticks/scenario at 50 Hz; headless tick path, no fire/growth workload"
    );
    run_scenario(Scenario::Clustered, steady_ticks)?;
    run_scenario(Scenario::Spread, steady_ticks)?;
    Ok(())
}

fn validate_tick_count(steady_ticks: usize) -> io::Result<()> {
    if !(MIN_STEADY_TICKS..=MAX_STEADY_TICKS).contains(&steady_ticks) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("server-perf requires {MIN_STEADY_TICKS}..={MAX_STEADY_TICKS} steady ticks"),
        ));
    }
    Ok(())
}

fn run_scenario(scenario: Scenario, steady_ticks: usize) -> io::Result<()> {
    let setup_started = Instant::now();
    let temp = TempSaveDir::create()?;
    let mut state = server_state(SEED, temp.path.clone())?;
    let (setup, warmed_chunks) = prepare(&mut state, scenario)?;
    let setup_elapsed = setup_started.elapsed();

    // The setup has no measured ticks, but reset explicitly so future setup
    // additions cannot contaminate steady-state percentiles or utilization.
    state.metrics = MetricsRecorder::new();
    println!(
        "{} setup: {:.3}s total ({} authoritative chunks made resident, {} players, {} seeded airborne drops); measured wall clock excludes setup",
        scenario.name(),
        setup_elapsed.as_secs_f64(),
        warmed_chunks,
        PLAYER_COUNT,
        PLAYER_COUNT * fixture::DROP_HEIGHTS.len(),
    );

    let steady_started = Instant::now();
    let mut deadline = steady_started + FIXED_STEP;
    let mut drains = DrainTotals::default();
    for tick_number in 1..=steady_ticks {
        let wait = deadline.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            thread::sleep(wait);
        }
        let now = Instant::now();
        state.tick_backlog = now
            .saturating_duration_since(deadline)
            .as_nanos()
            .checked_div(FIXED_STEP.as_nanos())
            .unwrap_or_default()
            .min(u64::MAX as u128) as u64;
        let inputs = tick_inputs(&state, &setup, scenario, tick_number)?;
        super::runtime::tick_with_inputs(
            &mut state,
            TickId::new(tick_number as u64),
            now,
            Vec::new(),
            inputs,
        )?;
        drain_outbound(&setup.receivers, &mut drains);
        deadline += FIXED_STEP;
    }
    let measured_wall = steady_started.elapsed();
    report::print_report(scenario, steady_ticks, measured_wall, &state, &drains);
    let scheduled_actions = (steady_ticks / ACTION_INTERVAL) * PLAYER_COUNT;
    let accepted_floor = scheduled_actions * 3 / 4;
    if drains.accepted_actions < accepted_floor as u64 {
        return Err(io::Error::other(format!(
            "{} scenario accepted {} durable actions; expected at least {accepted_floor} of {scheduled_actions} scheduled",
            scenario.name(),
            drains.accepted_actions,
        )));
    }
    // `State` owns disk and chunk worker threads. Drop it before its isolated
    // save directory is removed by `TempSaveDir`.
    drop(state);
    drop(temp);
    Ok(())
}
