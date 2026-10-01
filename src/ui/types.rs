//! Public data passed between input, UI layout, and rendering.
use crate::inventory::{SLOTS, Stack};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    Playing,
    Inventory,
    Container,
    Actions,
    Admin,
    Pause,
    Settings,
    Graphics,
    Character,
    Package,
    Joining,
    JoinFailed,
}

impl UiScreen {
    pub(crate) fn uses_egui(self) -> bool {
        matches!(
            self,
            Self::Inventory
                | Self::Container
                | Self::Actions
                | Self::Admin
                | Self::Pause
                | Self::Settings
                | Self::Graphics
                | Self::Character
                | Self::Package
                | Self::Joining
                | Self::JoinFailed
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingId {
    Sensitivity,
    FieldOfView,
    ViewDistance,
    UiScale,
    Lighting,
    PostProcessing,
    Characters,
    Exposure,
    Bloom,
    BloomStrength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiControl {
    Action(u8),
    InventorySearch,
    HotbarSlot(u8),
    InventorySlot(u8),
    KilnSlot(u8),
    AdminItem(u8),
    AdminPrev,
    AdminNext,
    AdminRun,
    AdminBindings,
    AdminBindingRow(u8),
    OpenAdmin,
    Resume,
    OpenSettings,
    OpenCharacter,
    ApplyCharacter,
    ToggleSettingsPage,
    Exit,
    Back,
    Decrease(SettingId),
    Increase(SettingId),
    ToggleFullscreen,
}

/// The command screen's display-only binding page uses the existing admin text
/// channel so preview frames need no additional fields or config access.
impl UiFrame<'_> {
    pub(crate) const BINDING_VIEW_PREFIX: &'static str = "\u{1}";
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
    pub authored_characters: bool,
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
            authored_characters: true,
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

/// Verified contiguous bytes received for this join; cache reuse transfers none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JoinProgress {
    pub received: u32,
    pub total: u32,
    pub cached: bool,
}

/// Values needed to draw a frame. Borrow status text to avoid per-frame string allocation.
#[derive(Clone, Debug)]
pub struct UiFrame<'a> {
    pub(crate) package_ui: Option<&'a super::authored::Session>,
    pub(crate) join_address: Option<&'a str>,
    pub(crate) join_progress: Option<JoinProgress>,
    pub screen: UiScreen,
    pub selected_slot: usize,
    pub inventory: [Option<Stack>; SLOTS],
    pub inventory_source: Option<u8>,
    pub inventory_search: &'a str,
    pub kiln: Option<crate::protocol::workstation::WorkstationView>,
    pub container_screen: Option<std::sync::Arc<bloxgloom_host_api::InventoryScreen>>,
    pub action_panel: Option<bloxgloom_host_api::actions::Panel>,
    pub kiln_source: Option<u8>,
    pub admin_enabled: bool,
    pub admin_page: usize,
    pub admin_input: &'a str,
    pub target: Option<[i32; 3]>,
    pub status: Option<&'a str>,
    pub debug: Option<UiDebug>,
    pub settings: UiSettings,
    pub(crate) character: Option<CharacterPanel>,
    pub hovered: Option<UiControl>,
}

impl Default for UiFrame<'_> {
    fn default() -> Self {
        Self {
            package_ui: None,
            join_address: None,
            join_progress: None,
            screen: UiScreen::Playing,
            selected_slot: 0,
            inventory: std::array::from_fn(|_| None),
            inventory_source: None,
            inventory_search: "",
            kiln: None,
            container_screen: None,
            action_panel: None,
            kiln_source: None,
            admin_enabled: false,
            admin_page: 0,
            admin_input: "",
            target: None,
            status: None,
            debug: None,
            settings: UiSettings::default(),
            character: None,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct CharacterPanel {
    pub cosmetics: [u8; 4],
    pub recipe: Option<crate::appearance::CharacterRecipe>,
    pub can_apply: bool,
    pub pending: bool,
    pub status: &'static str,
    pub clip: u8,
    pub time: f32,
    pub preview: Option<egui::TextureId>,
}
