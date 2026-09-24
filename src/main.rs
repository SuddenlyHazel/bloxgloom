mod client;
mod config;
mod lighting;
mod preview;
mod protocol;
mod raycast;
mod render;
mod server;
mod storage;
mod ui;
mod world;

fn main() {
    if let Err(error) = run() {
        eprintln!("bloxgloom: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {
            let (addr, _server) = server::start_local_server(0xB10C_6100, "world-v3".into())?;
            client::run_client(&addr.to_string())?;
        }
        Some("server") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            let save_dir = args.next().unwrap_or_else(|| "world-v3".to_string());
            server::run_server(&addr, 0xB10C_6100, save_dir.into())?;
        }
        Some("client") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            client::run_client(&addr)?;
        }
        Some("preview") => {
            let path = args.next().unwrap_or_else(|| "preview.png".to_string());
            let x = args
                .next()
                .map(|value| value.parse::<i32>())
                .transpose()?
                .unwrap_or(0);
            let z = args
                .next()
                .map(|value| value.parse::<i32>())
                .transpose()?
                .unwrap_or(0);
            if args.next().is_some() {
                return Err("usage: preview [output.png] [world-x] [world-z]".into());
            }
            preview::render_preview(std::path::Path::new(&path), x, z)?;
            println!("wrote {path}");
        }
        Some("ui-preview") => {
            let directory = args.next().unwrap_or_else(|| "ui-previews".to_string());
            preview::render_ui_previews(std::path::Path::new(&directory))?;
            println!("wrote UI previews to {directory}");
        }
        Some("lighting-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "lighting-previews".to_string());
            if args.next().is_some() {
                return Err("usage: lighting-preview [output-dir]".into());
            }
            preview::render_lighting_previews(std::path::Path::new(&directory))?;
            println!("wrote lighting previews to {directory}");
        }
        Some("perf") => {
            let steady_frames = args
                .next()
                .map(|value| value.parse::<usize>())
                .transpose()?
                .unwrap_or(preview::PERF_STEADY_FRAMES);
            let radius = args
                .next()
                .map(|value| value.parse::<u8>())
                .transpose()?
                .unwrap_or(preview::PERF_RADIUS);
            let bounced = match args.next().as_deref() {
                None | Some("voxel") => false,
                Some("bounced") => true,
                _ => return Err("usage: perf [steady-frames] [view-radius] [voxel|bounced]".into()),
            };
            if args.next().is_some() {
                return Err("usage: perf [steady-frames] [view-radius] [voxel|bounced]".into());
            }
            preview::run_perf_benchmark(steady_frames, radius, bounced)?;
        }
        Some(other) => {
            return Err(format!("unknown command {other:?}; use `server [address] [save-dir]`, `client [address]`, `preview [output.png]`, `ui-preview [output-dir]`, or `perf [steady-frames] [view-radius]`").into());
        }
    }
    Ok(())
}
