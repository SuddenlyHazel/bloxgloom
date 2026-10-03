//! Public model identity and bounded baked-model appearance. Catalog admission
//! performs the final validation against the selected rig's frozen controls.
use bloxgloom_host_api::entity::{MAX_VISUAL_BYTES, VisualSchema, VisualState};
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PackagedAppearance {
    pub model: u32,
    pub visual: VisualState,
}
impl PackagedAppearance {
    fn broad_schema() -> &'static VisualSchema {
        static SCHEMA: std::sync::OnceLock<VisualSchema> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| VisualSchema {
            clips: vec![String::new(); 256],
            clip_loops: vec![false; 256],
            variants: vec![(String::new(), vec![String::new(); 32]); 16],
            layers: vec![String::new(); 32],
            tints: vec![String::new(); 16],
        })
    }
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut bytes = self.model.to_le_bytes().to_vec();
        bytes.extend(
            self.visual
                .encode(Self::broad_schema())
                .expect("validated packaged player appearance"),
        );
        bytes
    }
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        if !(4..=4 + MAX_VISUAL_BYTES).contains(&bytes.len()) {
            return None;
        }
        Some(Self {
            model: u32::from_le_bytes(bytes[..4].try_into().ok()?),
            visual: VisualState::decode(&bytes[4..], Self::broad_schema()).ok()?,
        })
    }
    pub(crate) fn durable(mut self) -> Self {
        self.visual.playback = None;
        self.visual.sample_tick = 0;
        self.visual.sequence = 0;
        self
    }
}
