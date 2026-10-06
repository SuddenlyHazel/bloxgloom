//! Preserve raw binary32 evidence even when exact production stores agree.
use super::{POINTS, Row};

pub(super) fn export(
    sources: [&str; 2],
    points: [[u32; 4]; POINTS],
    raw: [&[Vec<Row>; 3]; 2],
    stored: [&[Vec<Row>; 3]; 2],
    banks: &[Vec<Row>; 3],
) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "bloxgloom-primary-packets-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (variant, source) in ["original", "counted"].into_iter().zip(sources) {
        std::fs::write(directory.join(format!("{variant}.wgsl")), source).unwrap();
    }
    for (variant, (raw, stored)) in ["original", "counted"]
        .into_iter()
        .zip(raw.into_iter().zip(stored))
    {
        for (index, rows) in raw.iter().enumerate() {
            std::fs::write(
                directory.join(format!("{variant}-raw-mode{}.f32le", index + 3)),
                bytemuck::cast_slice(rows),
            )
            .unwrap();
        }
        for (index, rows) in stored.iter().enumerate() {
            std::fs::write(
                directory.join(format!("{variant}-stored-mode{}.f32le", index + 6)),
                bytemuck::cast_slice(rows),
            )
            .unwrap();
        }
    }
    for (index, rows) in banks.iter().enumerate() {
        std::fs::write(
            directory.join(format!("counted-bank{index}.f32le")),
            bytemuck::cast_slice(rows),
        )
        .unwrap();
    }
    std::fs::write(
        directory.join("packet-layout.txt"),
        format!(
            "64 rows of eight little-endian float32 (two vec4 MRTs), row-major8x8. Modes3..5 are unquantized production function results; modes6..8 are actual binary16 physics storage values promoted tofloat32, keeping RNG halves and class/age controls unquantized. Banks0..2 have raw HDR xyz +five integer counters. Blocks are even-aligned2x2 at points {points:?}. Shader strings retained exactly. This is sparse attribution, no frame timing or visual acceptance.\n"
        ),
    )
    .unwrap();
    println!(
        "RAW AND PRODUCTION-STORED PRIMARY EVIDENCE: {}",
        directory.display()
    );
}

fn ordered(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits | 0x8000_0000
    } else {
        !bits
    }
}

pub(super) fn raw_difference(old: &[Row], new: &[Row], label: &str) {
    assert_eq!(old.len(), new.len());
    let mut changed = [0_usize; 8];
    let mut ulps = [0_u32; 8];
    let mut relative = [0_f32; 8];
    for (index, (old, new)) in old.iter().zip(new).enumerate() {
        if old
            .iter()
            .flatten()
            .zip(new.iter().flatten())
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            println!("RAW FP32 difference {label} sample={index} original={old:?} counted={new:?}");
        }
        for (field, (a, b)) in old.iter().flatten().zip(new.iter().flatten()).enumerate() {
            changed[field] += usize::from(a.to_bits() != b.to_bits());
            ulps[field] = ulps[field].max(ordered(*a).abs_diff(ordered(*b)));
            relative[field] =
                relative[field].max((a - b).abs() / a.abs().max(b.abs()).max(f32::MIN_POSITIVE));
        }
    }
    println!(
        "RAW FP32 {label}: changed field counts={changed:?}; maxULP={ulps:?}; max relative={relative:?}; raw bit identity is not the production-format pass criterion"
    );
}

pub(super) fn quantization_contract(rows: &[Row]) {
    let expected = [
        0x0000_u16, 0x8000, 0x3c00, 0xbc00, 0x3c00, 0x3c01, 0x3c02, 0x7bff, 0x0400, 0x0001, 0x0000,
        0x0001, 0x8001, 0x8000, 0x03ff, 0xc001,
    ];
    for (index, row) in rows.iter().enumerate() {
        for bits in &row[1][..2] {
            assert_eq!(*bits, f32::from(expected[index % expected.len()]));
        }
    }
    println!(
        "GPU binary16 quantization: exact independent signed/ties-to-even/tiny/normal-boundary/max-finite16-case oracle PASS"
    );
}
