//! Continuous snow retention and wind-scoured ground in cold regions.
//! WFC perimeter tiles describe other biomes' mosaics; they are not snow cover.
use super::{BlockId, Column, GRAVEL, SNOW, STONE, materials, noise2};

#[cfg(test)]
mod tests;

pub(super) fn surface(x: i64, z: i64, column: Column, seed: u64) -> BlockId {
    // Broad drifts with smaller irregular margins, all in absolute coordinates.
    // Colder/higher ground retains more snow; exposed slopes retain less.
    let drift =
        noise2(x, z, 48, seed ^ 0x6b17_90d3) * 0.75 + noise2(x, z, 13, seed ^ 0x381d_f25a) * 0.25;
    let cold = ((-0.23 - column.temperature) / 0.65).clamp(0.0, 1.0);
    let altitude = ((column.height as f64 - 60.0) / 28.0).clamp(0.0, 1.0);
    let exposure = column.slope.clamp(0.0, 1.5);
    if drift + cold * 0.35 + altitude * 0.2 - exposure * 0.3 > 0.02 {
        return SNOW;
    }
    // Independent bare geology avoids exchanging white perimeter strips for
    // gravel perimeter strips. Soil pockets survive on sheltered low slopes.
    let geology = noise2(x, z, 61, seed ^ 0x9572_eca1);
    if column.rocky || geology > 0.32 {
        STONE
    } else if geology < -0.22 && column.slope < 0.3 {
        materials::palette().soils[0]
    } else {
        GRAVEL
    }
}
