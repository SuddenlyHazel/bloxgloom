//! Offline package material/effect acceptance through production GPU pipelines.
use super::*;

pub fn render_visual_previews(
    directory: &Path,
    root: &Path,
    state_key: &str,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let state = crate::content::catalog()
        .state_by_key(state_key)
        .ok_or("unknown block state")?;
    for (name, scene) in [
        ("material", PreviewScene::Block(state)),
        ("composition", PreviewScene::Effect),
    ] {
        pollster::block_on(render_previews_with_packages(
            vec![PreviewOutput {
                path: directory.join(format!("{name}.png")),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            scene,
            Some(root),
        ))?;
    }
    Ok(())
}

pub(super) struct Resources {
    pub material: Option<crate::render::custom::Gpu>,
    pub effect: Option<Arc<crate::render::effects::Prepared>>,
    updates: Vec<crate::render::parameters::Update>,
}
impl Resources {
    pub fn prepare(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        root: &Path,
        opaque: &mut wgpu::RenderPipeline,
        cutout: &mut wgpu::RenderPipeline,
    ) -> Result<Self, Box<dyn Error>> {
        let snapshot = crate::server::PackageSnapshot::discover(root)
            .map_err(|e| format!("visual preview: {e:?}"))?;
        let bundle = Arc::clone(snapshot.client_bundle());
        let mut startup = crate::client::startup::prepare(bundle.clone())?;
        let updates = startup.parameters.take_updates();
        let material = if let Some(source) = bundle.material() {
            let prepared = source.resolve(crate::content::catalog())?;
            let ((o, c, _, _, _), mut gpu) = render::create_custom_voxel_pipeline(
                device,
                queue,
                render::post::HDR_FORMAT,
                crate::content::catalog(),
                &prepared,
            )?;
            *opaque = o;
            *cutout = c;
            for update in &updates {
                gpu.set(&update.resource, &update.name, &update.value)?;
            }
            Some(gpu)
        } else {
            None
        };
        Ok(Self {
            material,
            effect: bundle.effect().cloned(),
            updates,
        })
    }
    pub fn apply_effect_updates(&self, post: &mut render::post::PostProcess) -> Result<(), String> {
        for update in &self.updates {
            post.set_parameter(update)?;
        }
        Ok(())
    }
}
