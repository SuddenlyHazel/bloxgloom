//! Test-only sparse primary attribution from the current production frame inputs.
//! No fields, shader resources or hooks from this module exist in release builds.
mod draw;
mod evidence;
mod shader;
mod tests;
use super::Gpu;
use std::cell::RefCell;

const WIDTH: u32 = 8;
const HEIGHT: u32 = 8;
const POINTS: usize = 16;
type Row = [[f32; 4]; 2];

#[derive(Default)]
struct State {
    captured: bool,
    counts: Vec<[u32; 15]>,
    classes: Vec<[f32; 2]>,
}
thread_local! {
    // A scope on the invoking ignored-test thread, not a release environment
    // flag or a global renderer setting. Parallel tests cannot activate it.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}
struct Scope;
impl Scope {
    fn enter() -> Self {
        STATE.with(|state| assert!(state.borrow_mut().replace(State::default()).is_none()));
        Self
    }
    fn finish(self) -> State {
        STATE.with(|state| state.borrow_mut().take().unwrap())
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with(|state| {
            state.borrow_mut().take();
        });
    }
}

impl Gpu {
    /// Returns true only in the ignored-test scope, bypassing the full-frame
    /// transport after using the real raster/immutable-copy/deformation prelude.
    pub(super) fn primary_cost_probe(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame_group: &wgpu::BindGroup,
        materials: &wgpu::BindGroup,
    ) -> bool {
        STATE.with(|state| {
            let mut borrowed = state.borrow_mut();
            let Some(state) = borrowed.as_mut() else { return false; };
            if state.captured {return true;}
            // GPU-owned original depth/MRTs and current deformed triangles,
            // not reconstructed/synthetic raster inputs, precede every probe.
            let prelude = std::mem::replace(encoder, device.create_command_encoder(&Default::default()));
            queue.submit([prelude.finish()]);
            let size = self.history[0].texture().size();
            assert!(size.width >= 8 && size.height >= 8);
            let points = std::array::from_fn::<_, POINTS, _>(|index| {
                let xs = [0.08, 0.30, 0.53, 0.83];
                let ys = [0.14, 0.29, 0.51, 0.82];
                let x = ((size.width as f32 * xs[index % 4]) as u32).min(size.width-2)/2*2;
                let y = ((size.height as f32 * ys[index / 4]) as u32).min(size.height-2)/2*2;
                [x,y,0,0]
            });
            assert_eq!(crate::render::post::HDR_FORMAT, wgpu::TextureFormat::Rgba16Float);
            for view in [&self.history[0], &self.history_geometry[0], &self.current_correction] {
                assert_eq!(view.texture().format(), wgpu::TextureFormat::Rgba16Float);
            }
            assert_eq!(self.primary_transmission[0].texture().format(), super::lobes::format(self.water_lobes));
            assert!(matches!(self.primary_transmission[0].texture().format(), wgpu::TextureFormat::R16Float | wgpu::TextureFormat::Rgba16Float));
            let raw = self.water_reconstruction.is_some();
            let base = super::shaders::transport_for_modes(self.water_lobes, raw);
            let old_source = shader::source(base.clone(), false);
            let counted_source = shader::source(base, true);
            let material_layout = self.trace.get_bind_group_layout(1);
            let input = draw::Inputs {device, queue, frame: frame_group, materials,
                dynamic: &self.dynamic.group, layouts: [&self.layout,
                    &material_layout, &self.dynamic.layout]};
            let reference = draw::Probe::new(&input, &old_source, points);
            let counted = draw::Probe::new(&input, &counted_source, points);
            // Read and preserve EVERY packet before an equality assertion.
            // Real targets store binary16; raw f32 differences remain visible
            // independently and are never hidden by a numerical tolerance.
            let old_raw: [Vec<Row>;3] = std::array::from_fn(|i| reference.run(&input, i as u32+3));
            let new_raw: [Vec<Row>;3] = std::array::from_fn(|i| counted.run(&input, i as u32+3));
            let old_stored: [Vec<Row>;3] = std::array::from_fn(|i| reference.run(&input, i as u32+6));
            let new_stored: [Vec<Row>;3] = std::array::from_fn(|i| counted.run(&input, i as u32+6));
            let banks: [Vec<Row>;3] = std::array::from_fn(|mode| counted.run(&input, mode as u32));
            evidence::export([&old_source,&counted_source],points,[&old_raw,&new_raw],[&old_stored,&new_stored],&banks);
            for (i,label) in ["HDR/radial/RNG/class/age", "geometry/T", "current/RNG"].into_iter().enumerate() {
                evidence::raw_difference(&old_raw[i],&new_raw[i],label);
            }
            evidence::quantization_contract(&reference.run(&input,9));
            evidence::quantization_contract(&counted.run(&input,9));
            let old=&old_raw[0];
            for row in old {
                for value in &row[0][..3] {assert!(value.is_finite());}
                for value in &row[1][..2] {assert!(value.is_finite() && value.fract()==0.0 && (0.0..=65535.0).contains(value));}
            }
            for index in 0..(WIDTH*HEIGHT) as usize {
                let counts: [u32;15] = std::array::from_fn(|counter| {
                    let row = &banks[counter/5][index];
                    let value = if counter%5==0 {row[0][3]} else {row[1][counter%5-1]};
                    assert!(value.is_finite() && (0.0..16_777_216.0).contains(&value) && value.fract()==0.0);
                    value as u32
                });
                let block = index%WIDTH as usize/2 + 4*(index/WIDTH as usize/2);
                let coordinate = [points[block][0]+index as u32%2,
                    points[block][1]+(index as u32/WIDTH)%2];
                let row = old[index];
                let rng = row[1][0] as u32 | ((row[1][1] as u32)<<16);
                println!(
                    "ACTUAL PRIMARY lowres={coordinate:?} target={}x{} HDR={:?} rng={rng:#010x} class={} age={} counters={counts:?}",
                    size.width, size.height, &row[0][..3], row[1][2], row[1][3]
                );
                state.classes.push([row[1][2],row[1][3]]);state.counts.push(counts);
            }
            let mut same=true;
            for (i,label) in ["stored HDR / exact RNG/class/age", "stored geometry/T", "stored current / exact RNG"].into_iter().enumerate() {
                same &= exact(&old_stored[i], &new_stored[i],label);
            }
            assert!(same,"counter instrumentation changed actual production-format stores or exact RNG/class/age; all raw and stored packets exported before failure");
            state.captured=true;
            true
        })
    }
}
fn exact(old: &[Row], new: &[Row], label: &str) -> bool {
    let mut same = true;
    assert_eq!(old.len(), new.len());
    for (index, (a, b)) in old.iter().zip(new).enumerate() {
        for (x, y) in a.iter().flatten().zip(b.iter().flatten()) {
            if x.to_bits() != y.to_bits() {
                eprintln!("EXACT STORED mismatch {label} sample={index}: {a:?}/{b:?}");
                same = false;
            }
        }
    }
    same
}
