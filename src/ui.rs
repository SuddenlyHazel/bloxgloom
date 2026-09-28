//! Screen-space UI data, hit testing, and the compact GPU overlay renderer.

pub(crate) mod authored;
mod draw;
mod inventory_search;
mod layout;
mod renderer;
mod types;

pub(crate) use inventory_search::InventorySearch;
pub use layout::UiLayout;
pub(crate) use renderer::UiRenderer;
#[allow(unused_imports)] // Keep the existing ui::UiRect path available to callers.
pub use types::UiRect;
pub use types::{SettingId, UiControl, UiDebug, UiFrame, UiScreen, UiSettings};

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;
