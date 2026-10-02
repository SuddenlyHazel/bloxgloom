//! Bounded terrain query geometry, validated before any host read.
use crate::gameplay::{Cell, Error};
pub const MAX_QUERY_CELLS: usize = 64;
pub fn box_cells(origin: Cell, size: [u8; 3]) -> Result<Vec<Cell>, Error> {
    let count = size.iter().map(|n| usize::from(*n)).product::<usize>();
    if size.contains(&0) || count > MAX_QUERY_CELLS {
        return Err(Error::Invalid("terrain query requires 1..64 cells".into()));
    }
    for axis in 0..3 {
        origin[axis]
            .checked_add(i32::from(size[axis]) - 1)
            .ok_or_else(|| Error::Invalid("terrain query coordinate overflow".into()))?;
    }
    let mut cells = Vec::with_capacity(count);
    for x in 0..size[0] {
        for y in 0..size[1] {
            for z in 0..size[2] {
                cells.push([
                    origin[0] + i32::from(x),
                    origin[1] + i32::from(y),
                    origin[2] + i32::from(z),
                ]);
            }
        }
    }
    Ok(cells)
}
#[cfg(test)]
mod tests;
