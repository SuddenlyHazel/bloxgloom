//! Safe respawn selection uses captured authoritative cells, never generated fallback.
use super::*;
impl WorldSnapshot<'_> {
    pub(super) fn find_respawn(&mut self) -> Result<[f32; 3], Error> {
        let start = self
            .spawn_anchor
            .ok_or_else(|| Error::Invalid("native spawn unavailable".into()))?;
        let catalog = self.world.catalog_arc();
        let mut unavailable = None;
        for y in catalog
            .player_rules()
            .spawn()
            .cached_feet_levels(start[1] as i32, crate::world::BEDROCK_Y)
        {
            let support = match self.block([0, y - 1, 0]) {
                Ok(v) => v,
                Err(Error::Unavailable(cell)) => {
                    unavailable = Some(cell);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let id = catalog
                .state_by_key(&support.state)
                .ok_or_else(|| Error::Host("invalid spawn block".into()))?;
            if catalog.block_flags(id) & crate::content::SOLID == 0 {
                continue;
            }
            let position = catalog.player_rules().spawn().feet(y);
            let blocked = catalog.player_rules().body().collides(position, |x, y, z| {
                self.block([x, y, z]).and_then(|block| {
                    catalog
                        .state_by_key(&block.state)
                        .map(|id| catalog.block_flags(id) & crate::content::SOLID != 0)
                        .ok_or_else(|| Error::Host("invalid spawn block".into()))
                })
            });
            match blocked {
                Ok(false) => return Ok(position),
                Ok(true) => {}
                Err(Error::Unavailable(cell)) => unavailable = Some(cell),
                Err(e) => return Err(e),
            }
        }
        Err(unavailable.map_or_else(
            || Error::Invalid("no safe respawn near origin".into()),
            Error::Unavailable,
        ))
    }
}
