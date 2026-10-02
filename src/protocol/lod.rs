//! Strictly bounded distant terrain wire encoding, also used for disposable caches.
use super::{Cursor, invalid};
use crate::content::{BlockStateId, Catalog};
use crate::lod::{Column, Interval, LodTile, Span, TileKey};
use std::io;

pub(super) fn write_key(out: &mut Vec<u8>, key: TileKey) -> io::Result<()> {
    if key.bounds().is_none() {
        return Err(invalid("invalid LOD tile coordinate"));
    }
    out.push(key.level);
    out.extend(key.x.to_le_bytes());
    out.extend(key.z.to_le_bytes());
    Ok(())
}
pub(super) fn read_key(c: &mut Cursor<'_>) -> io::Result<TileKey> {
    let key = TileKey {
        level: c.u8()?,
        x: c.i32()?,
        z: c.i32()?,
    };
    if key.bounds().is_none() {
        return Err(invalid("invalid LOD tile coordinate"));
    }
    Ok(key)
}
pub(crate) fn tile_len(tile: &LodTile) -> usize {
    9 + 8
        + 4
        + 2
        + tile
            .columns
            .iter()
            .map(|col| 4 + col.coverage.len() * 8 + col.spans.len() * 14)
            .sum::<usize>()
}
pub(super) fn write_tile(out: &mut Vec<u8>, tile: &LodTile, catalog: &Catalog) -> io::Result<()> {
    tile.validate(catalog)
        .map_err(|_| invalid("invalid LOD summary"))?;
    write_key(out, tile.key)?;
    out.extend(tile.revision.to_le_bytes());
    out.extend(tile.geometric_error.to_le_bytes());
    out.extend((tile.columns.len() as u16).to_le_bytes());
    for col in &tile.columns {
        out.extend((col.coverage.len() as u16).to_le_bytes());
        out.extend((col.spans.len() as u16).to_le_bytes());
        for v in &col.coverage {
            out.extend(v.bottom.to_le_bytes());
            out.extend(v.top.to_le_bytes());
        }
        for v in &col.spans {
            out.extend(v.bottom.to_le_bytes());
            out.extend(v.top.to_le_bytes());
            out.extend(v.state.0.to_le_bytes());
            out.push(v.sky);
            out.push(v.glow);
        }
    }
    Ok(())
}
pub(super) fn read_tile(c: &mut Cursor<'_>, catalog: &Catalog) -> io::Result<LodTile> {
    let key = read_key(c)?;
    let revision = c.u64()?;
    let geometric_error = c.u32()?;
    let count = c.u16()? as usize;
    if count != 1024 {
        return Err(invalid("invalid LOD column count"));
    }
    let mut columns = Vec::with_capacity(count);
    let mut total_spans = 0usize;
    let mut total_coverage = 0usize;
    for _ in 0..count {
        let coverage_count = c.u16()? as usize;
        let span_count = c.u16()? as usize;
        total_spans += span_count;
        total_coverage += coverage_count;
        if span_count > 32 || total_spans > 3072 || total_coverage > 2048 {
            return Err(invalid("LOD allocation cap exceeded"));
        }
        let mut coverage = Vec::with_capacity(coverage_count);
        let mut spans = Vec::with_capacity(span_count);
        for _ in 0..coverage_count {
            coverage.push(Interval {
                bottom: c.i32()?,
                top: c.i32()?,
            });
        }
        for _ in 0..span_count {
            spans.push(Span {
                bottom: c.i32()?,
                top: c.i32()?,
                state: BlockStateId(c.u32()?),
                sky: c.u8()?,
                glow: c.u8()?,
            });
        }
        columns.push(Column { coverage, spans });
    }
    let tile = LodTile {
        key,
        revision,
        columns,
        geometric_error,
    };
    tile.validate(catalog)
        .map_err(|_| invalid("invalid LOD summary"))?;
    Ok(tile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientMessage, ServerMessage};
    #[test]
    fn bounded_summary_wire_roundtrip_and_allocation_rejection() {
        let catalog = crate::content::catalog();
        let tile = LodTile {
            key: TileKey {
                level: 4,
                x: -1,
                z: 1,
            },
            revision: 19,
            geometric_error: 16,
            columns: vec![
                Column {
                    coverage: vec![Interval {
                        bottom: -64,
                        top: 80
                    }],
                    spans: vec![Span {
                        bottom: -64,
                        top: 20,
                        state: crate::world::STONE,
                        sky: 0,
                        glow: 0
                    }]
                };
                1024
            ],
        };
        let message = ServerMessage::LodTile {
            session: 17,
            request: 5,
            tile: tile.clone(),
        };
        let mut bytes = Vec::new();
        crate::protocol::write_server_with_catalog(&mut bytes, &message, catalog).unwrap();
        assert_eq!(bytes.len(), crate::protocol::server_wire_len(&message));
        match crate::protocol::read_server_with_catalog(bytes.as_slice(), catalog).unwrap() {
            ServerMessage::LodTile {
                session: 17,
                request: 5,
                tile: decoded,
            } => assert_eq!(decoded, tile),
            other => panic!("unexpected {other:?}"),
        }
        // First column counts follow frame/version/tag/session/request/tile header.
        let span_count = 4 + 2 + 16 + 9 + 8 + 4 + 2 + 2;
        bytes[span_count..span_count + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(crate::protocol::read_server_with_catalog(bytes.as_slice(), catalog).is_err());
        let mut bytes = Vec::new();
        crate::protocol::write_client_with_catalog(
            &mut bytes,
            &ClientMessage::LodRequest {
                request: 5,
                key: tile.key,
            },
            catalog,
        )
        .unwrap();
        assert_eq!(
            crate::protocol::read_client_with_catalog(bytes.as_slice(), catalog).unwrap(),
            ClientMessage::LodRequest {
                request: 5,
                key: tile.key
            }
        );
    }
}
