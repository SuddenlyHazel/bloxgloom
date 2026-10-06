//! Discard buried material boundaries without extruding surface artwork downward.
use super::{Column, Span};
use crate::content::{CUTOUT, Catalog, FLUID, OPAQUE};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

pub(super) fn condense(columns: &mut [Column], catalog: &Catalog) {
    for column in columns {
        let source = std::mem::take(&mut column.spans);
        // Only the highest opaque component is a landscape surface. Retaining
        // every buried cave ceiling's material cap would double topology spans.
        // Cutout crowns above a trunk must not steal its original bark cap.
        let surface = source
            .iter()
            .rposition(|s| catalog.block_flags(s.state) & OPAQUE != 0);
        let mut begin = 0;
        while begin < source.len() {
            let optical_class =
                catalog.block_flags(source[begin].state) & (FLUID | CUTOUT | OPAQUE);
            let fluid = optical_class & FLUID;
            let mut end = begin + 1;
            while end < source.len()
                && source[end - 1].top == source[end].bottom
                && catalog.block_flags(source[end].state) & (FLUID | CUTOUT | OPAQUE)
                    == optical_class
            {
                end += 1;
            }
            let run = &source[begin..end];
            let cap = *run.last().unwrap();
            if fluid != 0 || run.len() == 1 {
                // Preserve the established fluid interface/visibility model.
                let mut joined = cap;
                joined.bottom = run[0].bottom;
                joined.glow = run.iter().map(|s| s.glow).max().unwrap();
                push(&mut column.spans, joined);
            } else {
                let retain_cap = surface == Some(end - 1);
                let body = if retain_cap {
                    &run[..run.len() - 1]
                } else {
                    run
                };
                let mut material_thickness = BTreeMap::new();
                for span in body {
                    *material_thickness.entry(span.state).or_insert(0_i64) +=
                        i64::from(span.top) - i64::from(span.bottom);
                }
                // Equal thickness chooses the lower stable state ID, independent
                // of iteration order; never use hash-map iteration as geology.
                let state = material_thickness
                    .into_iter()
                    .max_by_key(|(state, thickness)| (*thickness, std::cmp::Reverse(state.0)))
                    .unwrap()
                    .0;
                push(
                    &mut column.spans,
                    Span {
                        bottom: run[0].bottom,
                        top: if retain_cap { cap.bottom } else { cap.top },
                        state,
                        // The exposed cap must not inject skylight through its
                        // opaque roof into the retained solid body/cave sides.
                        sky: body.iter().map(|s| s.sky).max().unwrap(),
                        glow: run.iter().map(|s| s.glow).max().unwrap(),
                    },
                );
                if retain_cap {
                    push(&mut column.spans, cap);
                }
            }
            begin = end;
        }
    }
}

fn push(spans: &mut Vec<Span>, span: Span) {
    if let Some(last) = spans.last_mut()
        && last.top == span.bottom
        && last.state == span.state
        && last.sky == span.sky
        && last.glow == span.glow
    {
        last.top = span.top;
    } else {
        spans.push(span);
    }
}
