//! Shared package admission policy for discovery, declarations and delivery.
//! File bytes, decoded resources, execution heaps and per-tick work have distinct
//! budgets; a package must fit every applicable local and installation bound.
use std::time::Duration;

pub const MAX_PACKAGES: usize = 64;
pub const MAX_MODULES_PER_PACKAGE: usize = 256;
pub const MAX_MODULES: usize = 1024;
pub const MAX_ASSETS_PER_PACKAGE: usize = 256;
pub const MAX_ASSETS: usize = 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 40 * 1024 * 1024;

pub const BLOCKS_PER_PACKAGE: usize = 256;
pub const ITEMS_PER_PACKAGE: usize = 512;
pub const TEXTURES_PER_PACKAGE: usize = 256;
pub const SYSTEMS_PER_PACKAGE: usize = 8;
pub const GENERATORS_PER_PACKAGE: usize = 8;

pub const STARTUP_WALL_TIME: Duration = Duration::from_millis(250);
pub const STARTUP_INTERRUPTS: u64 = 50_000;
pub const STARTUP_MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const INSTALLATION_WALL_TIME: Duration = Duration::from_secs(10);
pub const CLIENT_PREPARATION_WALL_TIME: Duration = Duration::from_secs(10);
pub const GENERATION_SCRIPT_WALL_TIME: Duration = Duration::from_millis(100);
