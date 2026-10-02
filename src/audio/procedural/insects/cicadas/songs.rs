//! NoiseMachine species phrase data (MIT, Copyright 2026 kvmet).
use crate::audio::rain_tuning::CicadaSpecies;
#[derive(Clone, Copy)]
pub(super) struct Song {
    pub click: f32,
    pub q: f32,
    pub rate: [f32; 2],
    pub duty: f32,
    pub drop: f32,
    pub count: [u32; 2],
    pub fade: f32,
    pub hold: [f32; 2],
    pub throb: f32,
    pub gap: f32,
}
macro_rules! song {
    ($click:expr,$q:expr,$rate:expr,$duty:expr,$drop:expr,$count:expr,$fade:expr,$hold:expr,$throb:expr,$gap:expr) => {
        Song {
            click: $click,
            q: $q,
            rate: $rate,
            duty: $duty,
            drop: $drop,
            count: $count,
            fade: $fade,
            hold: $hold,
            throb: $throb,
            gap: $gap,
        }
    };
}
pub(super) fn song(species: CicadaSpecies) -> Song {
    use CicadaSpecies::*;
    match species {
        DogDay => song!(
            300.0,
            6.0,
            [1.0, 1.0],
            0.0,
            0.0,
            [0, 0],
            1.0,
            [10.0, 18.0],
            0.4,
            20.0
        ),
        Minminzemi => song!(
            400.0,
            20.0,
            [3.0, 3.0],
            0.7,
            -0.04,
            [5, 15],
            1.0,
            [1.0, 2.0],
            0.2,
            8.0
        ),
        Higurashi => song!(
            500.0,
            30.0,
            [8.0, 6.0],
            0.5,
            0.05,
            [20, 40],
            0.3,
            [0.0, 0.0],
            0.0,
            15.0
        ),
        Aburazemi => song!(
            450.0,
            4.0,
            [1.0, 1.0],
            0.0,
            0.0,
            [0, 0],
            1.0,
            [5.0, 20.0],
            0.1,
            10.0
        ),
        Niiniizemi => song!(
            500.0,
            15.0,
            [1.0, 1.0],
            0.0,
            0.0,
            [0, 0],
            1.0,
            [10.0, 30.0],
            0.05,
            10.0
        ),
        Kumazemi => song!(
            400.0,
            5.0,
            [4.0, 4.0],
            0.6,
            0.0,
            [20, 40],
            1.0,
            [0.0, 0.0],
            0.0,
            10.0
        ),
        Pharaoh => song!(
            300.0,
            10.0,
            [1.0, 1.0],
            0.0,
            0.0,
            [0, 0],
            1.0,
            [1.0, 3.0],
            0.0,
            5.0
        ),
        ScissorGrinder => song!(
            300.0,
            6.0,
            [5.0, 5.0],
            0.8,
            0.0,
            [50, 100],
            1.0,
            [0.0, 0.0],
            0.0,
            20.0
        ),
        CigaleGrise => song!(
            400.0,
            8.0,
            [8.0, 8.0],
            0.4,
            0.0,
            [80, 200],
            1.0,
            [0.0, 0.0],
            0.0,
            10.0
        ),
        GreenGrocer => song!(
            450.0,
            8.0,
            [1.0, 1.0],
            0.0,
            0.0,
            [0, 0],
            1.0,
            [15.0, 30.0],
            0.15,
            15.0
        ),
    }
}
