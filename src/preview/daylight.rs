//! Repeatable day/night comparisons through the production sky and voxel pipelines.
use super::*;

pub fn render_daylight_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, time, scene) in [
        ("sunrise", 0, PreviewScene::Surface),
        ("noon", crate::daylight::INITIAL_MS, PreviewScene::Surface),
        (
            "sunset",
            crate::daylight::CYCLE_MS / 2,
            PreviewScene::Surface,
        ),
        (
            "midnight",
            crate::daylight::CYCLE_MS * 3 / 4,
            PreviewScene::Surface,
        ),
        (
            "cave-dark-noon",
            crate::daylight::INITIAL_MS,
            PreviewScene::Cave {
                lamp: false,
                bounced: false,
            },
        ),
        (
            "cave-dark-night",
            crate::daylight::CYCLE_MS * 3 / 4,
            PreviewScene::Cave {
                lamp: false,
                bounced: false,
            },
        ),
        (
            "cave-lamp-noon",
            crate::daylight::INITIAL_MS,
            PreviewScene::Cave {
                lamp: true,
                bounced: true,
            },
        ),
        (
            "cave-lamp-night",
            crate::daylight::CYCLE_MS * 3 / 4,
            PreviewScene::Cave {
                lamp: true,
                bounced: true,
            },
        ),
    ] {
        let mut outputs = vec![PreviewOutput {
            path: directory.join(format!("{name}.png")),
            width: 1000,
            height: 600,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }];
        if matches!(scene, PreviewScene::Surface) {
            let atmosphere = render::daylight::Atmosphere::at(time);
            let direction = if atmosphere.sun.y < 0.0 {
                -atmosphere.sun
            } else {
                atmosphere.sun
            };
            outputs.push(PreviewOutput {
                path: directory.join(format!("{name}-sky.png")),
                width: 1000,
                height: 600,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: Some((direction.z.atan2(direction.x), direction.y.asin())),
            });
        }
        pollster::block_on(render_previews_at(outputs, (0, 0), scene, None, time))?;
    }
    Ok(())
}
