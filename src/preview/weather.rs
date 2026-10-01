//! Weather comparisons through production sky, voxel lighting and rain pipelines.
use super::*;
pub fn render_weather_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, cloud, rain, flash, exposure, scene) in [
        ("clear", 0.0, 0.0, 0.0, 1.0, PreviewScene::Surface),
        ("rain", 0.75, 0.65, 0.0, 1.0, PreviewScene::Surface),
        ("storm", 1.0, 1.0, 0.0, 1.0, PreviewScene::Surface),
        ("lightning", 1.0, 1.0, 0.85, 1.0, PreviewScene::Surface),
        (
            "sheltered",
            1.0,
            1.0,
            0.0,
            0.0,
            PreviewScene::Cave {
                lamp: true,
                bounced: false,
            },
        ),
        (
            "sealed-cave-lightning",
            1.0,
            1.0,
            1.0,
            0.0,
            PreviewScene::Cave {
                lamp: false,
                bounced: false,
            },
        ),
    ] {
        let weather =
            render::weather::Presentation::new(cloud, rain, [3.0, 1.0], exposure, 17.0, flash);
        let outputs = vec![PreviewOutput {
            path: directory.join(format!("{name}.png")),
            width: 1000,
            height: 600,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }];
        pollster::block_on(render_previews_weather(
            outputs,
            (0, 0),
            scene,
            None,
            crate::daylight::INITIAL_MS,
            weather,
        ))?;
    }
    Ok(())
}
