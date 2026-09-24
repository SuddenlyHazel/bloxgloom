use super::*;

fn state(value: u32) -> BlockId {
    BlockId::new(value)
}

#[test]
fn uniform_and_local_palette_boundaries_round_trip() {
    let mut blocks = PalettedBlocks::uniform(state(70_000));
    assert!(matches!(blocks.view(), PaletteView::Uniform(_)));
    assert_eq!(blocks.len(), CHUNK_VOLUME);
    assert_eq!(blocks.get(CHUNK_VOLUME), None);
    for unique in 2..=257 {
        assert_eq!(
            blocks.set(unique - 1, state(70_000 + unique as u32)),
            Some(state(70_000))
        );
        assert_eq!(blocks.unique_states(), unique);
        assert_eq!(blocks[unique - 1], state(70_000 + unique as u32));
        if unique <= 256 {
            assert!(matches!(blocks.view(), PaletteView::Palette8 { .. }));
        } else {
            assert!(matches!(blocks.view(), PaletteView::Palette16 { .. }));
        }
    }
    assert_eq!(PalettedBlocks::from(blocks.to_vec()), blocks);
    assert_eq!(blocks.set(256, state(70_000)), Some(state(70_257)));
    assert_eq!(blocks.unique_states(), 256);
    assert!(matches!(blocks.view(), PaletteView::Palette8 { .. }));
    for index in 1..256 {
        blocks.set(index, state(70_000));
    }
    assert_eq!(blocks.unique_states(), 1);
    assert!(matches!(blocks.view(), PaletteView::Uniform(_)));
    assert!(blocks.iter().all(|&block| block == state(70_000)));
}

#[test]
fn full_palette_reuses_removed_entry_and_preserves_other_cells() {
    let source = (0..CHUNK_VOLUME)
        .map(|index| state(index as u32 + 100_000))
        .collect::<Vec<_>>();
    let mut blocks = PalettedBlocks::from(source.clone());
    assert_eq!(blocks.unique_states(), CHUNK_VOLUME);
    assert!(matches!(blocks.view(), PaletteView::Palette16 { .. }));
    blocks.set(3, state(4_000_000));
    assert_eq!(blocks.unique_states(), CHUNK_VOLUME);
    assert_eq!(blocks[3], state(4_000_000));
    assert_eq!(blocks[CHUNK_VOLUME - 1], source[CHUNK_VOLUME - 1]);
    blocks.set(0, source[1]);
    assert_eq!(blocks.unique_states(), CHUNK_VOLUME - 1);
    assert_eq!(blocks[0], source[1]);
    assert_eq!(blocks[1], source[1]);
    assert_eq!(blocks[CHUNK_VOLUME - 1], source[CHUNK_VOLUME - 1]);
}

#[test]
fn wrong_length_stays_inspectable_for_validation() {
    let mut blocks = PalettedBlocks::from(vec![state(1), state(2)]);
    assert_eq!(blocks.len(), 2);
    assert!(matches!(blocks.view(), PaletteView::InvalidLength(_)));
    assert_eq!(blocks.set(1, state(3)), Some(state(2)));
    assert_eq!(blocks.to_vec(), vec![state(1), state(3)]);
    assert_eq!(blocks.set(2, state(4)), None);
}

#[test]
fn row_copy_matches_flat_storage_in_each_mode() {
    for unique in [1, 2, 256, 257, CHUNK_VOLUME] {
        let flat = (0..CHUNK_VOLUME)
            .map(|index| state((index % unique) as u32 + 100))
            .collect::<Vec<_>>();
        let blocks = PalettedBlocks::from(flat.clone());
        let mut row = [state(0); 16];
        assert_eq!(blocks.copy_range_to(31..47, &mut row), Some(()));
        assert_eq!(&row, &flat[31..47]);
        assert_eq!(blocks.copy_range_to(0..17, &mut row), None);
    }
}
