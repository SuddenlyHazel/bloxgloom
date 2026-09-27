//! Inspect any registered legal state through the normal light/mesh/GPU path.
use super::*;

pub fn render_block_preview(key: &str, path: &Path) -> Result<(), Box<dyn Error>> {
    let state = crate::content::catalog()
        .state_by_key(key)
        .ok_or("unknown legal block state")?;
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Block(state),
    ))
}
