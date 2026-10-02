//! One shared vertex upload; visible cube indices are grouped by material.
use super::*;
struct Part {
    node: usize,
    indices: Vec<u32>,
}
pub(super) struct Source {
    pub material: usize,
    parts: Vec<Part>,
}
impl Source {
    pub(super) fn index_count(&self) -> usize {
        self.parts.iter().map(|p| p.indices.len()).sum()
    }
    pub(super) fn visible_indices(&self, visible: &[bool]) -> Vec<u32> {
        self.parts
            .iter()
            .filter(|p| visible[p.node])
            .flat_map(|p| p.indices.iter().copied())
            .collect()
    }
}
pub(super) fn pack(model: &Model) -> (Vec<Vertex>, Vec<Source>) {
    let mut vertices = Vec::new();
    let mut sources: Vec<_> = (0..model.materials.len())
        .map(|material| Source {
            material,
            parts: Vec::new(),
        })
        .collect();
    for primitive in &model.primitives {
        let first = vertices.len() as u32;
        vertices.extend_from_slice(&primitive.vertices);
        sources[primitive.material].parts.push(Part {
            node: primitive.node,
            indices: primitive.indices.iter().map(|i| first + i).collect(),
        });
    }
    sources.retain(|s| !s.parts.is_empty());
    (vertices, sources)
}

#[cfg(test)]
mod tests;
