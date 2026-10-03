//! Cheap vertical sky visibility for summaries, without lighting unknown gaps.
use super::Column;
use crate::content::{Catalog, OPAQUE};

pub(crate) fn assign(column: &mut Column, catalog: &Catalog) {
    let Some(coverage) = column.coverage.last() else {
        return;
    };
    let Some(highest) = column.spans.last() else {
        return;
    };
    if coverage.bottom > highest.top || coverage.top <= highest.top {
        return;
    }
    let mut sky: u8 = 15;
    for span in column.spans.iter_mut().rev() {
        if span.bottom < coverage.bottom {
            break;
        }
        span.sky = sky;
        if catalog
            .state(span.state)
            .is_some_and(|s| s.flags & OPAQUE != 0)
        {
            break;
        }
        // Cutout foliage transmits attenuated daylight; it is not a sealed roof.
        let thickness = (span.top - span.bottom).min(15) as u8;
        sky = sky.saturating_sub(thickness.saturating_mul(2));
    }
}
