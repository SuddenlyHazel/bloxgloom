//! Public data passed between input, UI layout, and rendering.
use crate::inventory::{SLOTS, Stack};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    Playing,
    Inventory,
    Kiln,
    Admin,
    Pause,
    Settings,
    Graphics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingId {
    Sensitivity,
    FieldOfView,
    ViewDistance,
    UiScale,
    Lighting,
    PostProcessing,
    Exposure,
    Bloom,
    BloomStrength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiControl {
    HotbarSlot(u8),
    InventorySlot(u8),
    KilnSlot(u8),
    AdminItem(u8),
    AdminPrev,
    AdminNext,
    AdminRun,
    OpenAdmin,
    Resume,
    OpenSettings,
    ToggleSettingsPage,
    Exit,
    Back,
    Decrease(SettingId),
    Increase(SettingId),
    ToggleFullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSettings {
    /// Mouse radians per physical pixel.
    pub sensitivity: f32,
    pub fov_degrees: f32,
    pub view_distance: u8,
    pub scale: f32,
    pub fullscreen: bool,
    pub bounced_gi: bool,
    pub post_processing: bool,
    pub exposure: f32,
    pub bloom_enabled: bool,
    pub bloom_strength: f32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale: 1.0,
            fullscreen: false,
            bounced_gi: false,
            post_processing: true,
            exposure: 1.0,
            bloom_enabled: true,
            bloom_strength: 0.12,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiDebug {
    pub position: [f32; 3],
    pub fps: f32,
    pub frame_ms: f32,
    pub visible_chunks: usize,
    pub cached_chunks: usize,
    pub latency_ms: Option<u32>,
}

/// Values needed to draw a frame. Borrow status text to avoid per-frame string allocation.
#[derive(Clone, Debug)]
pub struct UiFrame<'a> {
    pub screen: UiScreen,
    pub selected_slot: usize,
    pub inventory: [Option<Stack>; SLOTS],
    pub inventory_source: Option<u8>,
    pub kiln: Option<crate::protocol::workstation::WorkstationView>,
    pub kiln_source: Option<u8>,
    pub admin_enabled: bool,
    pub admin_page: usize,
    pub admin_input: &'a str,
    pub target: Option<[i32; 3]>,
    pub status: Option<&'a str>,
    pub debug: Option<UiDebug>,
    pub settings: UiSettings,
    pub hovered: Option<UiControl>,
}

impl Default for UiFrame<'_> {
    fn default() -> Self {
        Self {
            screen: UiScreen::Playing,
            selected_slot: 0,
            inventory: std::array::from_fn(|_| None),
            inventory_source: None,
            kiln: None,
            kiln_source: None,
            admin_enabled: false,
            admin_page: 0,
            admin_input: "",
            target: None,
            status: None,
            debug: None,
            settings: UiSettings::default(),
            hovered: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}
