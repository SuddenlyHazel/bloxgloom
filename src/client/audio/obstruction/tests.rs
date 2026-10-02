use super::*;

fn scene(mut material: impl FnMut(i32, i32, i32) -> world::BlockId) -> Job {
    let mut chunks = HashMap::new();
    for y in -1..=0 {
        for z in -1..=0 {
            for x in -1..=0 {
                let key = ChunkKey { x, y, z };
                let mut blocks = Vec::with_capacity(world::CHUNK_VOLUME);
                for ly in 0..16 {
                    for lz in 0..16 {
                        for lx in 0..16 {
                            blocks.push(material(x * 16 + lx, y * 16 + ly, z * 16 + lz));
                        }
                    }
                }
                chunks.insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
            }
        }
    }
    Job {
        generation: 7,
        listener: [-3.5, 1.5, 0.5],
        sources: vec![Source {
            id: 4,
            position: [4.5, 1.5, 0.5],
        }],
        catalog: Arc::new(Catalog::builtins()),
        chunks,
    }
}
fn value(job: &Job) -> Value {
    solve(job).values[0]
}
#[test]
fn walls_attenuate_high_frequencies_while_leaves_remain_porous() {
    let air = value(&scene(|_, _, _| world::AIR));
    let wall = value(&scene(
        |x, _, _| if x == 0 { world::STONE } else { world::AIR },
    ));
    let leaves = value(&scene(
        |x, _, _| if x == 0 { world::LEAVES } else { world::AIR },
    ));
    let wood = value(&scene(
        |x, _, _| if x == 0 { world::WOOD } else { world::AIR },
    ));
    assert_eq!(air.gain, 1.0);
    assert!(wall.gain < 0.3 && wall.lowpass_hz < 2_500.0);
    assert!(leaves.gain > 0.9 && leaves.lowpass_hz > 17_000.0);
    assert!(wood.gain > wall.gain && wood.gain < leaves.gain);
}
#[test]
fn aligned_and_offset_doorways_are_audible_and_closing_them_muffles() {
    let closed = value(&scene(
        |x, _, _| if x == 0 { world::STONE } else { world::AIR },
    ));
    let aligned = value(&scene(|x, y, z| {
        if x == 0 && !(y == 1 && z == 0) {
            world::STONE
        } else {
            world::AIR
        }
    }));
    let offset = value(&scene(|x, y, z| {
        if x == 0 && !(y == 1 && z == 2) {
            world::STONE
        } else {
            world::AIR
        }
    }));
    assert_eq!(aligned.gain, 1.0);
    assert!(
        offset.gain > closed.gain * 1.5,
        "closed {}, offset {}",
        closed.gain,
        offset.gain
    );
    assert!(offset.gain < aligned.gain);
}
#[test]
fn thick_walls_reduce_transmission_and_unknown_space_never_invents_openings() {
    let thin = value(&scene(
        |x, _, _| if x == 0 { world::STONE } else { world::AIR },
    ));
    let thick = value(&scene(|x, _, _| {
        if (0..=2).contains(&x) {
            world::STONE
        } else {
            world::AIR
        }
    }));
    assert!(thick.gain < thin.gain * 0.25);
    let mut unknown = scene(|_, _, _| world::AIR);
    unknown.chunks.clear();
    let blocked = value(&unknown);
    assert_eq!(blocked.gain, MIN_GAIN);
    assert!(blocked.lowpass_hz < 1_000.0);
    unknown.sources[0].position = [-3.0, 1.5, 0.5];
    assert!(
        value(&unknown).gain < 0.3,
        "unknown endpoint cells must obstruct too"
    );
}
#[test]
fn negative_chunk_seam_and_only_embedded_endpoint_cell_are_respected() {
    let mut job = scene(|x, _, _| {
        if x == -1 || x == 4 {
            world::STONE
        } else {
            world::AIR
        }
    });
    let wall = value(&job);
    assert!(wall.gain < 0.3, "negative seam wall must obstruct");
    job.chunks = scene(|x, _, _| if x == 4 { world::STONE } else { world::AIR }).chunks;
    assert_eq!(value(&job).gain, 1.0, "skip source's exact anchored voxel");
    job.chunks = scene(|x, _, _| {
        if x == 3 || x == 4 {
            world::STONE
        } else {
            world::AIR
        }
    })
    .chunks;
    assert!(value(&job).gain < 0.3, "adjacent wall must still obstruct");
}
#[test]
fn jobs_preserve_identity_and_have_fixed_ray_and_memory_bounds() {
    let mut job = scene(|x, _, _| if x == 0 { world::STONE } else { world::AIR });
    assert!(valid(&job));
    let (v, cells) = obstruction(&job, job.sources[0]);
    assert!(cells <= MAX_RAY_CELLS * 25);
    assert_eq!(v.id, 4);
    assert_eq!(v.position, job.sources[0].position);
    let result = solve(&job);
    assert_eq!(result.generation, 7);
    assert_eq!(result.listener, job.listener);
    job.sources.resize(MAX_SOURCES + 1, job.sources[0]);
    assert!(!valid(&job));
    job.sources.truncate(1);
    job.sources[0].position[0] = f32::NAN;
    assert!(!valid(&job));
    job.sources[0].position = [100., 1.5, 0.5];
    assert!(!valid(&job));
    job.sources[0].position = [4.5, 1.5, 0.5];
    let mut mismatched = job.chunks.values().next().unwrap().as_ref().clone();
    mismatched.key = ChunkKey {
        x: 99,
        y: 99,
        z: 99,
    };
    job.chunks
        .insert(ChunkKey { x: 1, y: 0, z: 0 }, Arc::new(mismatched));
    assert!(!valid(&job));
}

#[test]
fn worker_channels_are_bounded_and_real_worker_returns_snapshot_identity() {
    // An undrained capacity-one channel deterministically refuses a second job.
    let (sender, _jobs) = mpsc::sync_channel(1);
    let (_results, receiver) = mpsc::sync_channel(1);
    let bounded = Worker { sender, receiver };
    assert!(bounded.try_submit(scene(|_, _, _| world::AIR)));
    assert!(!bounded.try_submit(scene(|_, _, _| world::AIR)));

    let worker = Worker::spawn().unwrap();
    assert!(worker.try_submit(scene(|x, _, _| {
        if x == 0 { world::STONE } else { world::AIR }
    })));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let result = loop {
        if let Some(result) = worker.poll() {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not return its bounded job"
        );
        thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(result.generation, 7);
    assert_eq!(result.listener, [-3.5, 1.5, 0.5]);
    assert_eq!(result.values.len(), 1);
    assert_eq!(result.values[0].id, 4);
    assert!(result.values[0].gain < 0.3);
}

#[test]
#[ignore = "explicit acoustic worker throughput measurement"]
fn worker_throughput() {
    // A single blocking voxel has all twelve bend waypoints open, exercising
    // every alternative ray rather than an infinite wall's cheap rejection.
    let mut job = scene(|x, y, z| {
        if x == 0 && y == 1 && z == 0 {
            world::STONE
        } else {
            world::AIR
        }
    });
    job.sources = (0..MAX_SOURCES)
        .map(|id| Source {
            id: id as u64,
            position: [4.5, 1.5, 0.5],
        })
        .collect();
    assert!(valid(&job));
    let (_, cells) = obstruction(&job, job.sources[0]);
    assert!(
        cells > 100,
        "fixture should exercise alternative paths: {cells}"
    );
    const ITERATIONS: usize = 200;
    let started = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        let result = std::hint::black_box(solve(std::hint::black_box(&job)));
        assert_eq!(result.values.len(), MAX_SOURCES);
    }
    let elapsed = started.elapsed();
    println!(
        "sound obstruction: {} sources, <=25 rays/source, {cells} visited cells/source, {ITERATIONS} jobs in {:.3}s; {:.1}us/job ({:.1}us/source)",
        MAX_SOURCES,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1_000_000.0 / ITERATIONS as f64,
        elapsed.as_secs_f64() * 1_000_000.0 / (ITERATIONS * MAX_SOURCES) as f64
    );
}

#[test]
fn coincident_unknown_endpoints_do_not_invent_clear_transmission() {
    let mut job = scene(|_, _, _| world::AIR);
    job.sources[0].position = job.listener;
    assert_eq!(value(&job).gain, 1.0);
    job.chunks.clear();
    assert_eq!(value(&job).gain, MIN_GAIN);
}

#[test]
fn terrain_profiles_muffle_fixture_motor_in_production_mixer() {
    use crate::audio::{Clip, Command, Controls, Mixer, SAMPLE_RATE};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/rain-collector/packages/rain/assets/sounds/motor.wav");
    let clip = Arc::new(Clip::load(&path).unwrap());
    let cases = [
        ("open", scene(|_, _, _| world::AIR)),
        (
            "stone wall",
            scene(|x, _, _| if x == 0 { world::STONE } else { world::AIR }),
        ),
        (
            "offset doorway",
            scene(|x, y, z| {
                if x == 0 && !(y == 1 && z == 2) {
                    world::STONE
                } else {
                    world::AIR
                }
            }),
        ),
        (
            "leaves",
            scene(|x, _, _| if x == 0 { world::LEAVES } else { world::AIR }),
        ),
    ];
    let mut rendered = Vec::with_capacity(cases.len());
    let mut energies = Vec::with_capacity(cases.len());
    for (label, job) in cases {
        let profile = value(&job);
        let mut mixer = Mixer::new(42);
        mixer.set_controls(Controls {
            master: 1.0,
            effects: 1.0,
            ambient: 0.0,
            ..Controls::default()
        });
        assert!(mixer.command(Command::Listener {
            position: job.listener,
            yaw: 0.0
        }));
        assert!(mixer.command(Command::PlayObstructed {
            clip: clip.clone(),
            position: profile.position,
            gain: 1.0,
            pitch: 1.0,
            looping: true,
            id: profile.id,
            transmission: profile.gain,
            lowpass_hz: profile.lowpass_hz,
        }));
        let mut frames = vec![[0.0; 2]; SAMPLE_RATE as usize * 2];
        for block in frames.chunks_mut(137) {
            mixer.render(block);
        }
        let energy: f64 = frames[SAMPLE_RATE as usize..]
            .iter()
            .flatten()
            .map(|sample| {
                assert!(sample.is_finite() && sample.abs() < 1.0);
                f64::from(*sample).powi(2)
            })
            .sum();
        println!(
            "{label}: transmission {:.3}, cutoff {:.0}Hz, motor energy {energy:.6}",
            profile.gain, profile.lowpass_hz
        );
        energies.push(energy);
        rendered.push(frames);
    }
    assert!(energies[0] > 0.0001, "fixture must actually be audible");
    assert!(
        energies[1] < energies[0] * 0.15,
        "stone must substantially muffle motor"
    );
    assert!(
        energies[2] > energies[1] * 2.0,
        "offset opening must carry real motor audio"
    );
    assert!(
        energies[2] < energies[0] * 0.7,
        "doorway detour remains sheltered"
    );
    assert!(
        energies[3] > energies[0] * 0.7,
        "leaves remain acoustically porous"
    );
    if let Some(path) = std::env::var_os("BLOXGLOOM_OBSTRUCTION_WAV") {
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: SAMPLE_RATE,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for frames in rendered {
            for sample in frames.into_iter().flatten() {
                writer.write_sample(sample).unwrap();
            }
            for _ in 0..SAMPLE_RATE / 4 {
                writer.write_sample(0.0f32).unwrap();
                writer.write_sample(0.0f32).unwrap();
            }
        }
        writer.finalize().unwrap();
        println!(
            "wrote {}: open, stone wall, offset doorway, leaves; 2 seconds each, 250ms gaps",
            path.display()
        );
    }
}
