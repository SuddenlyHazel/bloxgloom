//! Optional source-default water comparison, with private sqrt-space blending.
mod composition;
mod inputs;
mod mips;
pub(crate) use inputs::Inputs;
pub(super) struct Reference {
    pub(super) inputs: Inputs,
    noise: crate::render::sky::ReferenceNoise,
    composition: composition::Composition,
}
impl Reference {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let noise = crate::render::sky::ReferenceNoise::new(device);
        Self {
            inputs: Inputs::from_noise(device, &noise),
            noise,
            composition: composition::Composition::new(device),
        }
    }
    pub(super) fn begin(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        opaque_depth: &wgpu::TextureView,
    ) -> wgpu::TextureView {
        self.noise.upload(encoder);
        let target = self.composition.begin(device, encoder, scene, opaque_depth);
        self.inputs.opaque_depth = opaque_depth.clone();
        self.inputs.reflection = self.composition.reflection.as_ref().unwrap().clone();
        target
    }
    pub(super) fn front_depth(&self) -> Option<&wgpu::TextureView> {
        self.composition.front_depth.as_ref()
    }
    pub(super) fn finish(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
    ) {
        self.composition.finish(device, encoder, scene);
    }
}

pub(super) fn shader(lod: bool) -> String {
    let (group, first) = if lod { (0, 3) } else { (1, 1) };
    let sky = include_str!("../sky/camera.wgsl")
        .split("@group")
        .next()
        .unwrap();
    let clouds = include_str!("../sky/reference_clouds.wgsl")
        .lines()
        .filter(|line| !line.starts_with("@group"))
        .collect::<Vec<_>>()
        .join("\n");
    // Water calls the source reflection=true cloud case: shorter distance fade.
    let clouds = clouds.replace("fade_end=240.0/fog", "fade_end=80.0/fog");
    format!(
        "{sky}\n@group({group}) @binding({first}) var bg_reference_noise:texture_2d<f32>;\n@group({group}) @binding({}) var bg_reference_noise_sampler:sampler;\nstruct WaterReferenceFrame {{sky:SkyCamera,properties:vec4f,view_projection:mat4x4f,inverse_view_projection:mat4x4f,eye:vec4f}};\n@group({group}) @binding({}) var<uniform> water_reference:WaterReferenceFrame;\n@group({group}) @binding({}) var bg_water_opaque_depth:texture_depth_2d;\n@group({group}) @binding({}) var bg_water_reflection_image:texture_2d<f32>;\n@group({group}) @binding({}) var bg_water_reflection_sampler:sampler;\n{clouds}\n{}\n{}\n{}",
        first + 1,
        first + 2,
        first + 3,
        first + 4,
        first + 5,
        include_str!("reference/reflections.wgsl"),
        include_str!("reference/specular.wgsl"),
        include_str!("reference/surface.wgsl")
    )
}

#[cfg(test)]
#[path = "reference/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reference/wave_tests.rs"]
mod wave_tests;

pub(super) fn source(source: &str, lod: bool, reference: bool) -> String {
    if !reference {
        return source.to_owned();
    }
    let source = if lod {
        source.replace("    return bg_water_surface(v.color,", "    if BG_FOG_REFERENCE {return bg_reference_water_surface(v.normal,v.sky,glow,v.local+vec3f(tile.origin.xyz),v.relative,front,bg_sun_visibility(receiver),v.position.xy);}\n    return bg_water_surface(v.color,")
    } else {
        source.replace("    return bg_water_surface(v.color,", "    if BG_FOG_REFERENCE {return bg_reference_water_surface(v.normal,v.light.x,v.light.y,v.world,v.world-camera.eye.xyz,front,bg_sun_visibility(receiver),v.position.xy);}\n    return bg_water_surface(v.color,")
    };
    let source = if lod {
        source
            .replace(
                "@vertex fn vs_main(v: In) -> Out {",
                "fn bg_lod_reference_vertex(v: In) -> Out {",
            )
            .replace(
                "fn bg_lod_coverage(v: Out)",
                &format!(
                    "{}\nfn bg_lod_coverage(v: Out)",
                    include_str!("reference/lod_vertices.wgsl")
                ),
            )
    } else {
        source.replace("@vertex fn vs(v:Input)->Output { return Output(camera.view_projection*vec4f(v.position,1.0),v.position,v.normal,v.color,v.light); }", include_str!("reference/near_vertex.wgsl"))
    };
    format!("{}\n{source}", shader(lod))
}

#[cfg(test)]
#[path = "reference/reflection_tests.rs"]
mod reflection_tests;

#[cfg(test)]
#[path = "reference/depth_tests.rs"]
mod depth_tests;
