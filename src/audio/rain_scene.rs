//! Bounded, immutable acoustic geometry sent from resident world snapshots.
use std::sync::Arc;

pub(crate) const MAX_RAIN_TILES: usize = 16 * 16 * 3;
pub(crate) const RAIN_MATERIALS: usize = 10;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum RainMaterial {
    Water,
    Dirt,
    Leaf,
    Concrete,
    Glass,
    Metal,
    Plastic,
    Asphalt,
    AsphaltRoof,
    Wood,
}
impl RainMaterial {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "water" => Self::Water,
            "dirt" => Self::Dirt,
            "leaf" => Self::Leaf,
            "stone" | "concrete" => Self::Concrete,
            "glass" => Self::Glass,
            "metal" => Self::Metal,
            "plastic" => Self::Plastic,
            "asphalt" => Self::Asphalt,
            "roof" => Self::AsphaltRoof,
            "wood" => Self::Wood,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Habitat {
    #[default]
    None,
    Ground,
    Canopy,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RainTile {
    /// World-space face centre. A tile represents one square metre.
    pub centre: [f32; 3],
    pub material: RainMaterial,
    pub habitat: Habitat,
    /// Outward horizontal normal for a wall; [0, 0] means a horizontal top.
    pub normal: [f32; 2],
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RainScene {
    pub tiles: Vec<RainTile>,
}
impl RainScene {
    pub fn valid(&self) -> bool {
        self.tiles.len() <= MAX_RAIN_TILES
            && self.tiles.iter().all(|tile| {
                tile.centre
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 16_000_000.0)
                    && matches!(
                        tile.normal,
                        [0.0, 0.0] | [-1.0, 0.0] | [1.0, 0.0] | [0.0, -1.0] | [0.0, 1.0]
                    )
            })
    }
    /// Reproducible material comparison using the same geometry as live rain.
    pub fn patch(material: RainMaterial) -> Arc<Self> {
        Arc::new(Self {
            tiles: (-8..8)
                .flat_map(|z| {
                    (-8..8).map(move |x| RainTile {
                        centre: [x as f32 + 0.5, 0.0, z as f32 + 0.5],
                        material,
                        habitat: match material {
                            RainMaterial::Dirt => Habitat::Ground,
                            RainMaterial::Leaf => Habitat::Canopy,
                            _ => Habitat::None,
                        },
                        normal: [0.0; 2],
                    })
                })
                .collect(),
        })
    }
}
