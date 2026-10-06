//! Optional conservative 16m XZ candidates. Vertical interval precedence stays in WGSL.
const MAGIC: u32 = 0x5758_5a31;
const MAX_BYTES: usize = 1024 * 1024;

pub(super) fn append(data: &mut Vec<u32>, directory: usize, count: usize, limit: u64) {
    if std::env::var("BLOXGLOOM_GI_VOLUME_INDEX").as_deref() == Ok("0") {
        return;
    }
    let Some(index) = build(data, directory, count, limit) else {
        return;
    };
    let offset = data.len();
    data.extend(index);
    data[directory + 6] = offset as u32;
    data[directory + 7] = MAGIC;
}

fn build(data: &[u32], directory: usize, count: usize, limit: u64) -> Option<Vec<u32>> {
    if count == 0 {
        return None;
    }
    let mut bounds = Vec::with_capacity(count);
    for tile in 0..count {
        let at = directory + tile * 8;
        let origin = [data[at] as i32 as f32, data[at + 1] as i32 as f32];
        let extent = data[at + 4] as f32;
        // Match the shader's rounded origin+extent, including very distant origins.
        let low = origin.map(|v| (v / 16.0).floor() as i64);
        let high = origin.map(|v| ((v + extent) / 16.0).ceil() as i64);
        if low.iter().zip(high).any(|(&a, b)| b <= a) {
            return None;
        }
        bounds.push((low, high));
    }
    let minimum =
        std::array::from_fn::<_, 2, _>(|axis| bounds.iter().map(|b| b.0[axis]).min().unwrap());
    let maximum =
        std::array::from_fn::<_, 2, _>(|axis| bounds.iter().map(|b| b.1[axis]).max().unwrap());
    let width = usize::try_from(maximum[0] - minimum[0]).ok()?;
    let height = usize::try_from(maximum[1] - minimum[1]).ok()?;
    let cells = width.checked_mul(height)?;
    let candidates = bounds.iter().try_fold(0usize, |sum, (low, high)| {
        let area = usize::try_from((high[0] - low[0]).checked_mul(high[1] - low[1])?).ok()?;
        sum.checked_add(area)
    })?;
    let words = 8usize
        .checked_add(cells.checked_mul(2)?)?
        .checked_add(candidates)?;
    let bytes = words.checked_mul(4)?;
    if bytes > MAX_BYTES
        || (data.len() as u64)
            .checked_mul(4)?
            .checked_add(bytes as u64)?
            > limit
    {
        return None;
    }
    let base = data.len();
    if base.checked_add(words)? > u32::MAX as usize {
        return None;
    }
    let mut result = vec![0u32; words];
    result[..8].copy_from_slice(&[
        minimum[0] as i32 as u32,
        minimum[1] as i32 as u32,
        width as u32,
        height as u32,
        (base + 8) as u32,
        0,
        0,
        0,
    ]);
    for (low, high) in &bounds {
        for z in low[1]..high[1] {
            for x in low[0]..high[0] {
                let cell = (x - minimum[0]) as usize + width * (z - minimum[1]) as usize;
                result[8 + cell * 2 + 1] += 1;
            }
        }
    }
    let mut cursor = 8 + cells * 2;
    for cell in 0..cells {
        result[8 + cell * 2] = (base + cursor) as u32;
        cursor += result[8 + cell * 2 + 1] as usize;
    }
    let mut used = vec![0usize; cells];
    for (tile, (low, high)) in bounds.iter().enumerate() {
        for z in low[1]..high[1] {
            for x in low[0]..high[0] {
                let cell = (x - minimum[0]) as usize + width * (z - minimum[1]) as usize;
                let slot = result[8 + cell * 2] as usize - base + used[cell];
                result[slot] = tile as u32;
                used[cell] += 1;
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests;
