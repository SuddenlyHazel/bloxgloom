use super::*;
pub(crate) const BYTES: usize = 57;
pub(crate) fn encode(s: WeatherSnapshot) -> Vec<u8> {
    let mut b = Vec::with_capacity(BYTES);
    b.extend(s.elapsed_ms.to_le_bytes());
    b.extend(s.seed.to_le_bytes());
    for v in [s.from.rain, s.from.cloud, s.from.wind] {
        b.extend(v.to_le_bytes())
    }
    b.push(s.to as u8);
    b.extend(s.transition_start_ms.to_le_bytes());
    b.extend(s.transition_duration_ms.to_le_bytes());
    b.extend(s.next_change_ms.to_le_bytes());
    b.extend(s.revision.to_le_bytes());
    b
}
pub(crate) fn decode(b: &[u8]) -> Option<WeatherSnapshot> {
    if b.len() != BYTES {
        return None;
    }
    let s = WeatherSnapshot {
        elapsed_ms: u64::from_le_bytes(b[0..8].try_into().ok()?),
        seed: u64::from_le_bytes(b[8..16].try_into().ok()?),
        from: WeatherValues {
            rain: f32::from_le_bytes(b[16..20].try_into().ok()?),
            cloud: f32::from_le_bytes(b[20..24].try_into().ok()?),
            wind: f32::from_le_bytes(b[24..28].try_into().ok()?),
        },
        to: WeatherKind::from_u8(b[28])?,
        transition_start_ms: u64::from_le_bytes(b[29..37].try_into().ok()?),
        transition_duration_ms: u32::from_le_bytes(b[37..41].try_into().ok()?),
        next_change_ms: u64::from_le_bytes(b[41..49].try_into().ok()?),
        revision: u64::from_le_bytes(b[49..57].try_into().ok()?),
    };
    s.valid().then_some(s)
}
