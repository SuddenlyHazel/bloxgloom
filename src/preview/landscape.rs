//! Fixed, unedited generated landscapes, rendered through production paths.
use super::*;

pub(super) mod horizon;

#[derive(Clone, Copy, Debug)]
pub(super) enum Shot {
    Meadow,
    Coast,
    Grove,
    Mountains,
}
impl Shot {
    pub(super) fn site(self) -> (i32, i32) {
        // Fixed seed 0xB10C6100. Unlike the authored showcase,
        // every block in these views comes from ordinary world generation.
        match self {
            Self::Meadow => (-2000, -2048),
            Self::Coast => (-656, -2048),
            Self::Grove => (-710, -2044),
            Self::Mountains => (-1024, -1840),
        }
    }
    pub(super) fn camera(self) -> (Vec3, Vec3) {
        let (x, z) = self.site();
        let height = world::terrain_height(i64::from(x), i64::from(z), SEED).max(16) as f32;
        let target = Vec3::new(
            x as f32 + 0.5,
            height
                + if matches!(self, Self::Grove) {
                    7.0
                } else {
                    3.0
                },
            z as f32 + 0.5,
        );
        let offset = match self {
            Self::Meadow => Vec3::new(28.0, 13.0, 34.0),
            Self::Coast => Vec3::new(38.0, 18.0, 32.0),
            Self::Grove => Vec3::new(30.0, 18.0, 36.0),
            Self::Mountains => Vec3::new(42.0, 22.0, 45.0),
        };
        let mut eye = target + offset;
        // Clearance follows actual generated blocks, including the tallest
        // nearby crowns. Ground height alone can place the lens inside a tree.
        let mut clearance = 16;
        for dx in [-4, 0, 4] {
            for dz in [-4, 0, 4] {
                clearance = clearance.max(surface_height(
                    eye.x.floor() as i32 + dx,
                    eye.z.floor() as i32 + dz,
                ));
            }
        }
        // Nearby crowns along the viewing direction must not fill the frame
        // even when the camera's own block column happens to be empty.
        for fraction in [0.15, 0.30] {
            let near = eye.lerp(target, fraction);
            clearance = clearance.max(surface_height(near.x.floor() as i32, near.z.floor() as i32));
        }
        eye.y = eye.y.max(clearance as f32 + 6.0);
        (eye, target)
    }
}

pub fn render_landscape_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let selected = std::env::var("BLOXGLOOM_LANDSCAPE_VIEW").ok();
    if selected
        .as_deref()
        .is_some_and(|view| !["meadow", "coast", "cherry-grove", "mountains"].contains(&view))
    {
        return Err(
            "BLOXGLOOM_LANDSCAPE_VIEW must be meadow, coast, cherry-grove, or mountains".into(),
        );
    }
    let mut settings = format!(
        "Generated landscapes; generator {}; seed {SEED:#x}; production voxel/water/shadow/postprocess paths; distant terrain horizon 512m\n1280x800; noon; installed material artwork; no terrain edits or saved world\nTemporal AA requested: {}\nGI requested: {}; target scale: {}; BSL reference mode: {}\n",
        world::TERRAIN_GENERATOR_VERSION,
        render::post::temporal_requested(),
        std::env::var("BLOXGLOOM_GI").unwrap_or_else(|_| "0".into()),
        std::env::var("BLOXGLOOM_GI_SCALE").unwrap_or_else(|_| "2".into()),
        std::env::var("BLOXGLOOM_BSL_REFERENCE").unwrap_or_else(|_| "enhanced".into()),
    );
    for (name, shot) in [
        ("01-meadow", Shot::Meadow),
        ("02-coast", Shot::Coast),
        ("03-cherry-grove", Shot::Grove),
        ("04-mountains", Shot::Mountains),
    ] {
        if selected.as_deref().is_some_and(|view| &name[3..] != view) {
            continue;
        }
        let (x, z) = shot.site();
        let (eye, target) = shot.camera();
        settings.push_str(&format!(
            "{name}: site=({x},{z}); eye={eye:?}; target={target:?}\n"
        ));
        pollster::block_on(render_previews_at(
            vec![PreviewOutput {
                path: directory.join(format!("{name}.png")),
                width: 1280,
                height: 800,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (x.div_euclid(16), z.div_euclid(16)),
            PreviewScene::Landscape(shot),
            None,
            crate::daylight::INITIAL_MS,
        ))?;
        println!("generated landscape capture: {name}");
    }
    fs::write(directory.join("capture-settings.txt"), settings)?;
    Ok(())
}
