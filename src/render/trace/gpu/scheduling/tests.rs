use super::*;
#[test]
fn odd_transport_extents_are_covered_once_with_global_derivative_quads() {
    for (width, height) in [(1, 1), (65, 129), (133, 71), (640, 400)] {
        for edge in [32, 64, 128] {
            let mut counts = vec![0u8; (width * height) as usize];
            for tile in tiles(width, height, edge) {
                assert_eq!(tile.x % 2, 0);
                assert_eq!(tile.y % 2, 0);
                assert!(tile.width * tile.height <= edge * edge);
                for y in tile.y..tile.y + tile.height {
                    for x in tile.x..tile.x + tile.width {
                        counts[(y * width + x) as usize] += 1;
                    }
                }
            }
            assert!(counts.iter().all(|&count| count == 1));
        }
        assert_eq!(
            tiles(width, height, 0),
            vec![Tile {
                x: 0,
                y: 0,
                width,
                height
            }]
        );
    }
}
