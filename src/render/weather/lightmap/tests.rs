use super::*;
#[test]
fn precipitation_light_samples_respect_negative_seams_and_unknown_coverage() {
    let mut cache = std::collections::HashMap::new();
    let (left, cell) = crate::world::world_to_chunk(-1, 2, 0);
    let mut values = vec![0; CHUNK_SIZE.pow(3)];
    values[Chunk::index(cell).unwrap()] = 12;
    cache.insert(left, values.into_boxed_slice());
    assert_eq!(sample(glam::Vec3::new(-0.01, 2.5, 0.5), &cache), 0.8);
    assert_eq!(sample(glam::Vec3::new(0.01, 2.5, 0.5), &cache), 0.0);
    assert_eq!(sample(glam::Vec3::new(-1.01, 2.5, 0.5), &cache), 0.0);
}
