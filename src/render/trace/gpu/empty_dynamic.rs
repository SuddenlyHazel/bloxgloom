//! Optional compile-time empty-actor specialization, certified by current uploaded nodes.
use super::{Gpu, lobes};
use crate::render::post::HDR_FORMAT;

pub(super) fn configured(water_lobes: bool) -> bool {
    let requested = std::env::var("BLOXGLOOM_GI_EMPTY_DYNAMIC").as_deref() == Ok("1");
    if requested && water_lobes {
        static WARNING: std::sync::Once = std::sync::Once::new();
        WARNING.call_once(|| eprintln!("certified-empty dynamic specialization is scoped to R16 transport; retaining generic split/raw pipeline"));
    }
    allowed(requested, water_lobes)
}

fn allowed(requested: bool, water_lobes: bool) -> bool {
    requested && !water_lobes
}

pub(super) fn source_for(source: &str, empty: bool) -> String {
    if !empty {
        return source.to_owned();
    }
    let mut output = source.to_owned();
    for (anchor, replacement) in [
        (
            "return dynamic_ray_cast_impl(origin,direction,limit,current,false);",
            "return current;",
        ),
        (
            "return dynamic_ray_cast_impl(origin,direction,limit,current,true);",
            "return current;",
        ),
        (
            "    if (hit.triangle&0x80000000u)!=0u {return dynamic_ray_surface(hit);}\n",
            "",
        ),
        (
            "let primary_actor=first.triangle!=0xffffffffu&&(first.triangle&0x80000000u)!=0u;",
            "let primary_actor=false;",
        ),
        ("if ray_dynamic_touched&&!primary_actor {", "if false {"),
        ("if (first.triangle&0x80000000u)!=0u {", "if false {"),
    ] {
        assert_eq!(
            source.matches(anchor).count(),
            1,
            "empty-dynamic source anchor changed: {anchor}"
        );
        output = output.replacen(anchor, replacement, 1);
    }
    output
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    source: &str,
    layout: &wgpu::BindGroupLayout,
    materials: &wgpu::BindGroupLayout,
    dynamic: &wgpu::BindGroupLayout,
    water_lobes: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("certified-empty dynamic transport"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("unchanged certified-empty transport inputs"),
        bind_group_layouts: &[Some(layout), Some(materials), Some(dynamic)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("certified-empty four MRT transport"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_transport"),
            compilation_options: Default::default(),
            targets: &[
                HDR_FORMAT,
                HDR_FORMAT,
                lobes::format(water_lobes),
                HDR_FORMAT,
            ]
            .map(|format| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            }),
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Gpu {
    pub(super) fn empty_dynamic_active(&self) -> bool {
        self.empty_trace.is_some() && self.dynamic.is_empty()
    }

    pub(super) fn transport_pipeline(&self) -> &wgpu::RenderPipeline {
        if self.empty_dynamic_active() {
            self.empty_trace.as_ref().unwrap()
        } else {
            &self.trace
        }
    }
}

#[cfg(test)]
mod tests;
