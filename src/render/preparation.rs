//! A single asynchronous GPU preparation candidate. Nothing becomes live until
//! every package pipeline has compiled; dropping a candidate retires its result.
use super::{Renderer, custom, effects, pipeline};
use std::{sync::mpsc, thread};

pub(crate) struct Ready {
    material: Option<(pipeline::VoxelPipelines, custom::Gpu)>,
    effect: Option<effects::Effect>,
    sun_pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

pub(crate) struct Preparation {
    result: mpsc::Receiver<Result<Ready, String>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Preparation {
    pub(crate) fn poll(&mut self) -> Option<Result<Ready, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("package GPU preparation worker stopped".into()))
            }
        }
    }
    // Only after the event loop closes. Live cancellation waits by polling.
    pub(crate) fn finish(mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Renderer {
    pub(crate) fn prepare_package_visuals(
        &self,
        material: Option<&custom::Prepared>,
        effect: Option<&effects::Prepared>,
    ) -> Result<Preparation, String> {
        Preparation::start(
            self.device.clone(),
            self.queue.clone(),
            self.catalog.clone(),
            material,
            effect,
        )
    }
    pub(crate) fn commit_package_visuals(&mut self, ready: Ready) {
        if let Some(((opaque, cutout, _, _, _), gpu)) = ready.material {
            self.pipeline = opaque;
            self.cutout_pipeline = cutout;
            self.material_gpu = Some(gpu);
        }
        if let Some(pipelines) = ready.sun_pipelines {
            self.sun_pipelines = pipelines;
        }
        if let Some(effect) = ready.effect {
            self.post.install_prepared_effect(&self.device, effect);
        }
    }
}
impl Preparation {
    fn start(
        device: wgpu::Device,
        queue: wgpu::Queue,
        catalog: std::sync::Arc<crate::content::Catalog>,
        material: Option<&custom::Prepared>,
        effect: Option<&effects::Prepared>,
    ) -> Result<Self, String> {
        let material = material.cloned();
        let effect = effect.cloned();
        let (tx, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("package-gpu-preparation".into())
            .spawn(move || {
                let ready = (|| {
                    let prepared_material = material;
                    let material = prepared_material
                        .as_ref()
                        .map(|m| {
                            pipeline::create_custom_voxel_pipeline(
                                &device,
                                &queue,
                                super::post::HDR_FORMAT,
                                &catalog,
                                m,
                            )
                        })
                        .transpose()?;
                    let effect = effect
                        .as_ref()
                        .map(|e| effects::Effect::prepare(&device, e))
                        .transpose()?;
                    let sun_pipelines = material.as_ref().map(|(pipelines, _)| {
                        pipeline::create_sun_shadow_pipelines(
                            &device,
                            &pipelines.0,
                            prepared_material.as_ref(),
                        )
                    });
                    Ok(Ready {
                        material,
                        effect,
                        sun_pipelines,
                    })
                })();
                let _ = tx.send(ready);
            })
            .map_err(|e| format!("package GPU preparation: {e}"))?;
        Ok(Preparation {
            result,
            worker: Some(worker),
        })
    }
}

#[cfg(test)]
mod tests;
