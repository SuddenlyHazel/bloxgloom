//! Native geometry and textures, with a BLAS built on a dedicated asset thread.
use glam::Vec3;
use std::sync::{Arc, OnceLock, mpsc};

#[derive(Clone)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
    pub part: u32,
}
#[derive(Clone)]
pub(crate) struct Material {
    /// 0 linear flat color, 1 native GLB, 2 builtin player, 3 catalog item.
    pub kind: u32,
    pub base: [f32; 4],
    pub image: Option<usize>,
    pub wrap: [u32; 2],
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub surface: u32,
    pub group: u32,
    pub catalog_layer: u32,
}
impl Material {
    pub fn flat(color: [f32; 3]) -> Self {
        Self {
            kind: 0,
            base: [color[0], color[1], color[2], 1.0],
            image: None,
            wrap: [0; 2],
            alpha_cutoff: -1.0,
            double_sided: false,
            surface: 0,
            group: 0,
            catalog_layer: 0,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}
#[derive(Clone)]
pub(super) struct Triangle {
    pub indices: [u32; 3],
    pub material: u32,
    pub part: u32,
}
pub(super) struct Node {
    pub first: u32,
    pub count: u32,
    pub escape: u32,
    pub right: u32,
    pub level: u32,
}
pub(crate) struct DynamicAsset {
    pub(super) vertices: Vec<Vertex>,
    pub(super) triangles: Vec<Triangle>,
    pub(super) materials: Vec<Material>,
    pub(super) images: Vec<Image>,
    pub(super) nodes: Vec<Node>,
    pub(super) levels: u32,
}
struct Build {
    vertices: Vec<Vertex>,
    triangles: Vec<([u32; 3], u32, u32)>,
    materials: Vec<Material>,
    images: Vec<Image>,
    ready: mpsc::SyncSender<Arc<DynamicAsset>>,
}
fn asset_worker() -> &'static mpsc::Sender<Build> {
    static WORKER: OnceLock<mpsc::Sender<Build>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Build>();
        std::thread::Builder::new()
            .name("ray-dynamic-asset".into())
            .spawn(move || {
                while let Ok(build) = receive.recv() {
                    let mut asset = DynamicAsset {
                        vertices: build.vertices,
                        triangles: build
                            .triangles
                            .into_iter()
                            .map(|(indices, material, part)| Triangle {
                                indices,
                                material,
                                part,
                            })
                            .collect(),
                        materials: build.materials,
                        images: build.images,
                        nodes: Vec::new(),
                        levels: 0,
                    };
                    if !asset.triangles.is_empty() {
                        asset.partition(0, asset.triangles.len(), 0);
                    }
                    let _ = build.ready.send(Arc::new(asset));
                }
            })
            .expect("dynamic asset worker");
        send
    })
}
impl DynamicAsset {
    pub fn build(
        vertices: Vec<Vertex>,
        triangles: Vec<([u32; 3], u32, u32)>,
        materials: Vec<Material>,
        images: Vec<Image>,
    ) -> Arc<Self> {
        // Startup/first admission does not perform BVH assembly on the window thread.
        let (ready, receive) = mpsc::sync_channel(1);
        asset_worker()
            .send(Build {
                vertices,
                triangles,
                materials,
                images,
                ready,
            })
            .expect("dynamic asset worker");
        receive.recv().expect("dynamic asset BLAS")
    }
    fn partition(&mut self, first: usize, count: usize, depth: u32) -> usize {
        self.levels = self.levels.max(depth + 1);
        let index = self.nodes.len();
        self.nodes.push(Node {
            first: first as u32,
            count: count as u32,
            escape: 0,
            right: 0,
            level: depth,
        });
        if count > 8 {
            let center = |triangle: &Triangle| {
                triangle
                    .indices
                    .iter()
                    .map(|i| Vec3::from_array(self.vertices[*i as usize].position))
                    .sum::<Vec3>()
                    / 3.0
            };
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for triangle in &self.triangles[first..first + count] {
                let c = center(triangle);
                low = low.min(c);
                high = high.max(c);
            }
            let extent = high - low;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            self.triangles[first..first + count]
                .sort_by(|a, b| center(a)[axis].total_cmp(&center(b)[axis]));
            let half = count / 2;
            self.partition(first, half, depth + 1);
            let right = self.partition(first + half, count - half, depth + 1);
            self.nodes[index].count = 0;
            self.nodes[index].right = right as u32;
        }
        self.nodes[index].escape = self.nodes.len() as u32;
        index
    }
}
