//! Natural, unedited water scenes rendered through the production fluid pass.
use super::*;
pub(crate) fn render_water_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let mut sites = std::collections::BTreeMap::<&str, (i32, i32, i64, i64)>::new();
    for z in (-512..=512).step_by(4) {
        for x in (-512..=512).step_by(4) {
            if let Some((level, bed, kind)) = world::water_feature(x, z, SEED) {
                let depth = level - bed;
                if depth >= 2
                    && sites.get(kind).is_none_or(|&(ox, oz, _, old)| {
                        (x * x + z * z, -depth)
                            < (
                                i64::from(ox) * i64::from(ox) + i64::from(oz) * i64::from(oz),
                                -old,
                            )
                    })
                {
                    sites.insert(kind, (x as i32, z as i32, level, depth));
                }
            }
        }
    }
    for kind in ["river", "lake", "pond"] {
        let &(x, z, level, depth) = sites.get(kind).ok_or("missing generated water feature")?;
        eprintln!("{kind}: x={x} z={z} surface={} depth={depth}", level + 1);
        pollster::block_on(render_previews(
            vec![PreviewOutput {
                path: directory.join(format!("{kind}.png")),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (x.div_euclid(16), z.div_euclid(16)),
            PreviewScene::Water { x, z, level },
        ))?;
    }
    Ok(())
}
