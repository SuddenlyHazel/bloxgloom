use crate::render::{Renderer, custom};

impl Renderer {
    pub(in crate::render) fn draw_sun_shadows(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(mut pass) = self.sun_shadows.begin(encoder) else {
            return;
        };
        let padding = self.material_gpu.as_ref().map_or(0.0, custom::Gpu::padding);
        pass.set_pipeline(&self.sun_pipelines.0);
        pass.set_bind_group(0, &self.sun_shadows.caster_group, &[]);
        pass.set_bind_group(1, &self.texture_group, &[]);
        if let Some(gpu) = &self.material_gpu {
            pass.set_bind_group(2, &gpu.group, &[]);
        }
        for (key, mesh) in &self.meshes {
            if self.sun_shadows.projection.contains_chunk(*key, padding)
                && let Some(opaque) = &mesh.opaque
            {
                pass.set_vertex_buffer(0, opaque.vertex.slice(..));
                pass.set_index_buffer(opaque.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..opaque.indices, 0, 0..1);
            }
        }
        if self.drop_index_count > 0 {
            pass.set_vertex_buffer(0, self.drop_vertices.slice(..));
            pass.set_index_buffer(self.drop_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.drop_index_count, 0, 0..1);
        }
        pass.set_pipeline(&self.sun_pipelines.1);
        for (key, mesh) in &self.meshes {
            if self.sun_shadows.projection.contains_chunk(*key, padding)
                && let Some(cutout) = &mesh.cutout
            {
                pass.set_vertex_buffer(0, cutout.vertex.slice(..));
                pass.set_index_buffer(cutout.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..cutout.indices, 0, 0..1);
            }
        }
        if self.drop_cutout_index_count > 0 {
            pass.set_vertex_buffer(0, self.drop_cutout_vertices.slice(..));
            pass.set_index_buffer(
                self.drop_cutout_indices.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.draw_indexed(0..self.drop_cutout_index_count, 0, 0..1);
        }
        self.avatars
            .draw_shadow(&mut pass, &self.sun_shadows.caster_group);
    }
}
