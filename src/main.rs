mod client;
mod config;
mod content;
mod inventory;
mod items;
mod lighting;
mod physics;
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
    content::install(content::Catalog::builtins())
        .map_err(|_| "content catalog was installed more than once")?;
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {
            let config_path = config::Config::default_path();
            let mut config = config::Config::load(&config_path);
            config.ensure_profile(&config_path)?;
            let (addr, server) = server::start_local_server_with_admin(
                0xB10C_6100,
                "world-v6".into(),
                config.profile,
            )?;
            let client_result = client::run_client_with_admin(&addr.to_string());
            let server_result = server.stop();
            client_result?;
            server_result?;
        }
        Some("server") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            let save_dir = args.next().unwrap_or_else(|| "world-v6".to_string());
            let admission_limit = args
                .next()
                .map(|value| value.parse::<usize>())
                .transpose()?;
            if args.next().is_some() {
                return Err("usage: server [address] [save-dir] [max-clients: 1..=256]".into());
            }
            if let Some(limit) = admission_limit {
                server::run_server_with_limit(&addr, 0xB10C_6100, save_dir.into(), limit)?;
            } else {
                server::run_server(&addr, 0xB10C_6100, save_dir.into())?;
            }
        }
        Some("server-perf") => {
            let first = args.next();
            if first.as_deref() == Some("tcp") {
                let mut clients = None;
                let mut ticks = None;
                let mut scene = None;
                while let Some(flag) = args.next() {
                    let value = args.next().ok_or("missing TCP benchmark option value")?;
                    match flag.as_str() {
                        "--clients" if clients.is_none() => clients = Some(value.parse::<usize>()?),
                        "--ticks" if ticks.is_none() => ticks = Some(value.parse::<usize>()?),
                        "--scene" if scene.is_none() => scene = Some(value),
                        _ => return Err(
                            "usage: server-perf tcp --clients N --ticks N --scene clustered|spread"
                                .into(),
                        ),
                    }
                }
                server::run_tcp_perf(
                    clients.ok_or("missing --clients")?,
                    ticks.ok_or("missing --ticks")?,
                    scene.as_deref().ok_or("missing --scene")?,
                )?;
            } else if first.as_deref() == Some("fire") {
                let measured_ticks = args
                    .next()
                    .ok_or("usage: server-perf fire <measured-ticks> [--warmup N]")?
                    .parse::<usize>()?;
                let warmup = match args.next() {
                    None => 300,
                    Some(flag) if flag == "--warmup" => args
                        .next()
                        .ok_or("missing fire benchmark warmup value")?
                        .parse::<usize>()?,
                    Some(_) => {
                        return Err("usage: server-perf fire <measured-ticks> [--warmup N]".into());
                    }
                };
                if args.next().is_some() {
                    return Err("usage: server-perf fire <measured-ticks> [--warmup N]".into());
                }
                server::run_fire_perf(measured_ticks, warmup)?;
            } else if first.as_deref() == Some("fire-cpu") {
                let mut workers = None;
                let mut iterations = None;
                while let Some(flag) = args.next() {
                    let value = args.next().ok_or("missing fire-cpu option value")?;
                    match flag.as_str() {
                        "--workers" if workers.is_none() => workers = Some(value.parse::<usize>()?),
                        "--iterations" if iterations.is_none() => {
                            iterations = Some(value.parse::<usize>()?)
                        }
                        _ => {
                            return Err(
                                "usage: server-perf fire-cpu --workers N --iterations N".into()
                            );
                        }
                    }
                }
                server::run_fire_cpu_perf(
                    workers.ok_or("missing --workers")?,
                    iterations.ok_or("missing --iterations")?,
                )?;
            } else {
                let steady_ticks = first
                    .map(|value| value.parse::<usize>())
                    .transpose()?
                    .unwrap_or(300);
                if args.next().is_some() {
                    return Err("usage: server-perf [steady-ticks (min 300)]".into());
                }
                server::run_perf_benchmark(steady_ticks)?;
            }
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
        Some("vegetation-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "vegetation-preview.png".to_string());
            if args.next().is_some() {
                return Err("usage: vegetation-preview [output.png]".into());
            }
            preview::render_vegetation_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
        }
        Some("drop-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "drop-preview.png".to_string());
            if args.next().is_some() {
                return Err("usage: drop-preview [output.png]".into());
            }
            preview::render_drop_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
        }
        Some("drop-animation-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "drop-animation-previews".to_string());
            if args.next().is_some() {
                return Err("usage: drop-animation-preview [output-dir]".into());
            }
            preview::render_drop_animation_previews(std::path::Path::new(&directory))?;
            println!("wrote drop animation previews to {directory}");
        }
        Some("avatar-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "avatar-preview.png".to_string());
            if args.next().is_some() {
                return Err("usage: avatar-preview [output.png]".into());
            }
            preview::render_avatar_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
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
            return Err(format!("unknown command {other:?}; use `server [address] [save-dir]`, `server-perf [steady-ticks]`, `client [address]`, `preview [output.png]`, `ui-preview [output-dir]`, or `perf [steady-frames] [view-radius]`").into());
        }
    }
    Ok(())
}
