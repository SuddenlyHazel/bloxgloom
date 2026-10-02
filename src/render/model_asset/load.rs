//! Validate a self-contained GLB before decoding bounded scene data.
use super::*;
mod clips;
mod geometry;
mod materials;
use std::collections::HashSet;

pub(super) fn load(bytes: &[u8], controls: Controls) -> Result<Model> {
    ensure(
        bytes.len() <= MAX_BYTES && bytes.starts_with(b"glTF"),
        "expected a GLB of at most 64 MiB",
    )?;
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| format!("invalid GLB: {e}"))?;
    let blob = g
        .blob
        .as_deref()
        .ok_or("GLB has no embedded binary buffer")?;
    ensure(
        g.extensions_required().next().is_none(),
        "required glTF extensions are not supported",
    )?;
    ensure(
        g.buffers().len() == 1
            && matches!(
                g.buffers().next().unwrap().source(),
                gltf::buffer::Source::Bin
            )
            && g.buffers().next().unwrap().length() <= blob.len(),
        "one embedded GLB buffer required",
    )?;
    ensure(
        g.nodes().len() <= 256 && g.meshes().len() <= 256 && g.skins().len() <= 32,
        "GLB node/mesh/skin limits exceeded",
    )?;
    for view in g.views() {
        ensure(
            view.offset()
                .checked_add(view.length())
                .is_some_and(|end| end <= blob.len()),
            "GLB buffer view out of bounds",
        )?;
    }
    for accessor in g.accessors() {
        ensure(
            accessor.count() <= 196608 && accessor.sparse().is_none(),
            "oversized or sparse accessor unsupported",
        )?;
        let view = accessor.view().ok_or("accessor must have a buffer view")?;
        let size = accessor.size();
        let stride = view.stride().unwrap_or(size);
        let end = accessor
            .count()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|v| v.checked_add(accessor.offset()))
            .and_then(|v| v.checked_add(size));
        ensure(
            stride >= size && end.is_some_and(|end| end <= view.length()),
            "GLB accessor out of bounds",
        )?;
    }
    let (nodes, order) = hierarchy(&g)?;
    let (materials, images) = materials::load(&g, blob)?;
    let (bindings, primitives) = geometry::load(&g, blob, &order, nodes.len(), materials.len())?;
    let clips = clips::load(&g, blob, &order)?;
    let model = Model {
        nodes,
        bindings,
        primitives,
        materials,
        images,
        clips,
        controls,
    };
    model.controls.validate(&model)?;
    model.sample(None, 0.0)?;
    Ok(model)
}
fn hierarchy(g: &gltf::Gltf) -> Result<(Vec<Node>, Vec<Option<usize>>)> {
    let source: Vec<_> = g.nodes().collect();
    let mut parents = vec![None; source.len()];
    for node in &source {
        for child in node.children() {
            ensure(
                parents[child.index()].replace(node.index()).is_none(),
                "GLB node has multiple parents",
            )?;
        }
    }
    let scene = g
        .default_scene()
        .or_else(|| g.scenes().next())
        .ok_or("GLB has no scene")?;
    let mut order = vec![None; source.len()];
    let mut nodes = Vec::new();
    let mut visited = HashSet::new();
    fn visit(
        node: gltf::Node<'_>,
        parent: Option<usize>,
        nodes: &mut Vec<Node>,
        order: &mut [Option<usize>],
        visited: &mut HashSet<usize>,
    ) -> Result<()> {
        ensure(
            visited.insert(node.index()),
            "cyclic or repeated scene node",
        )?;
        // Matrix nodes are valid static assets. Reject shear/perspective instead of
        // silently losing it while decomposing into animatable TRS.
        let source = Mat4::from_cols_array_2d(&node.transform().matrix());
        let (t, r, s) = node.transform().decomposed();
        let rest = Transform {
            translation: Vec3::from_array(t),
            rotation: Quat::from_array(r),
            scale: Vec3::from_array(s),
        };
        ensure(
            rest.translation.is_finite()
                && rest.rotation.is_finite()
                && rest.scale.is_finite()
                && rest.scale.min_element() > 1e-6
                && (rest.rotation.length_squared() - 1.0).abs() < 0.001
                && source.abs_diff_eq(rest.matrix(), 0.001),
            "GLB node needs finite nonsheared positive-scale TRS",
        )?;
        let index = nodes.len();
        order[node.index()] = Some(index);
        nodes.push(Node {
            name: named(node.name(), "node", node.index())?,
            parent,
            rest,
        });
        for child in node.children() {
            visit(child, Some(index), nodes, order, visited)?;
        }
        Ok(())
    }
    for root in scene.nodes() {
        ensure(parents[root.index()].is_none(), "scene root has a parent")?;
        visit(root, None, &mut nodes, &mut order, &mut visited)?;
    }
    ensure(!nodes.is_empty(), "GLB active scene is empty")?;
    Ok((nodes, order))
}
