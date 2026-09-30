//! Shared fixtures and focused integration suites for the server coordinator.

use super::*;

#[path = "tests/appearance.rs"]
mod appearance;
#[path = "tests/clients.rs"]
mod clients;
#[path = "tests/common.rs"]
mod common;
#[path = "tests/drop_pins.rs"]
mod drop_pins;
#[path = "tests/durable.rs"]
mod durable;
#[path = "tests/entity_sleep.rs"]
mod entity_sleep;
#[path = "tests/player_operations.rs"]
mod player_operations;
#[path = "tests/simulation.rs"]
mod simulation;
#[path = "tests/startup.rs"]
mod startup;

use common::*;
