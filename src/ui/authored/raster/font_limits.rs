//! A deliberately small TTF subset profile. Reject expansion-heavy font features
//! before fontdue enumerates cmap ranges or builds glyph outlines.
use super::*;

fn word(bytes: &[u8], at: usize) -> Result<usize> {
    Ok(u16::from_be_bytes(bytes.get(at..at + 2).ok_or(INVALID)?.try_into().unwrap()) as usize)
}
fn dword(bytes: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_be_bytes(bytes.get(at..at + 4).ok_or(INVALID)?.try_into().unwrap()) as usize)
}

pub(super) fn validate(face: &ttf_parser::Face<'_>) -> Result<()> {
    let count = face.number_of_glyphs() as usize;
    if count > 128 || face.is_variable() || !(512..=4096).contains(&face.units_per_em()) {
        return Err(INVALID);
    }
    let table = |name: &[u8; 4]| {
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(name))
            .ok_or(INVALID)
    };
    let cmap = table(b"cmap")?;
    let tables = word(cmap, 2)?;
    if tables == 0 || tables > 4 {
        return Err(INVALID);
    }
    for i in 0..tables {
        let at = dword(cmap, 4 + i * 8 + 4)?;
        let sub = cmap.get(at..).ok_or(INVALID)?;
        if word(sub, 0)? != 4 {
            return Err(INVALID);
        }
        let segments = word(sub, 6)? / 2;
        if segments == 0 || segments > 128 || word(sub, 2)? > 4096 {
            return Err(INVALID);
        }
        let mut previous = None;
        for s in 0..segments {
            let end = word(sub, 14 + s * 2)?;
            let start = word(sub, 16 + segments * 2 + s * 2)?;
            if start > end
                || previous.is_some_and(|p| start <= p)
                || !((start >= 32 && end <= 126) || (start == 65535 && end == 65535))
            {
                return Err(INVALID);
            }
            previous = Some(end);
        }
    }
    let head = table(b"head")?;
    let long = match word(head, 50)? {
        0 => false,
        1 => true,
        _ => return Err(INVALID),
    };
    let loca = table(b"loca")?;
    let glyf = table(b"glyf")?;
    let offset = |index| {
        if long {
            dword(loca, index * 4)
        } else {
            word(loca, index * 2).map(|n| n * 2)
        }
    };
    for i in 0..count {
        let start = offset(i)?;
        let end = offset(i + 1)?;
        let glyph = glyf.get(start..end).ok_or(INVALID)?;
        if glyph.is_empty() {
            continue;
        }
        // Negative contours denote compound glyphs: unsupported in this profile.
        let contours = word(glyph, 0)?;
        if contours > 64 || glyph.len() > 4096 {
            return Err(INVALID);
        }
        if contours > 0 && word(glyph, 10 + (contours - 1) * 2)? >= 512 {
            return Err(INVALID);
        }
    }
    // Kerning/shaping are not used; don't let unused tables cause extra parsing.
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"kern"))
        .is_some()
    {
        return Err(INVALID);
    }
    Ok(())
}
