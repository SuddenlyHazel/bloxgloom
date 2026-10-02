use super::*;
use crate::audio::rain_tuning::CicadaSpecies;
#[test]
fn every_species_has_a_distinct_finite_audible_phrase() {
    let mut energies = Vec::new();
    for species in CicadaSpecies::ALL {
        let mut synth = Cicadas::new(42);
        let mut c = CicadaProfile {
            species,
            ..Default::default()
        };
        c.tone.pitch_hz = species.pitch();
        c.tone.chorus = 0.0;
        synth.configure(c);
        synth.place(
            &[Some([3.0, 0.0, 0.0]); VOICES],
            [0.0; 3],
            0.0,
            Listener::default(),
        );
        let mut bus = Bus::default();
        let mut energy = 0.0f64;
        for _ in 0..44100 {
            synth.next(&[true; VOICES], true, 0.1, &mut bus);
            let v = bus.next();
            assert!(v.iter().all(|v| v.is_finite() && v.abs() < 10.0));
            energy += f64::from(v[0]).powi(2);
        }
        assert!(energy > 0.001, "{} was silent", species.label());
        energies.push(energy.to_bits());
    }
    energies.sort_unstable();
    energies.dedup();
    assert_eq!(energies.len(), 10);
}
