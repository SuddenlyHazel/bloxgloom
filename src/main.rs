mod appearance;
mod audio;
mod client;
mod config;
mod content;
mod daylight;
mod gameplay;
mod inventory;
mod items;
mod lighting;
mod lod;
mod logging;
mod physics;
mod preview;
mod protocol;
mod raycast;
mod render;
mod response_trace;
mod server;
mod storage;
mod ui;
mod weather;
mod world;

fn main() -> std::process::ExitCode {
    let _logging = match logging::init() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("could not initialize logging: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "Bloxgloom failed");
            // Returning lets the logging guard flush, including this final error.
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let catalog = content::Catalog::builtins();
    #[cfg(feature = "lifecycle-fixture")]
    let catalog = server::catalog_with_extension(catalog, &bloxgloom_lifecycle_fixture::Fixture)?;
    let catalog = if arguments.first().is_some_and(|command| {
        command == "creature-preview"
            || command == "inventory-preview"
            || command == "block-preview"
    }) && (arguments.len() == 4
        || (arguments
            .first()
            .is_some_and(|command| command == "creature-preview")
            && arguments.len() == 5))
    {
        server::package_catalog_for_preview(catalog, std::path::Path::new(&arguments[3]))?
    } else if arguments
        .first()
        .is_some_and(|command| command == "visual-preview")
        && arguments.len() == 4
    {
        server::package_catalog_for_preview(catalog, std::path::Path::new(&arguments[2]))?
    } else {
        catalog
    };
    let mut catalog = catalog;
    if arguments
        .first()
        .is_some_and(|command| command == "sandbox-preview")
    {
        preview::install_sandbox_materials(&mut catalog)?;
    }
    content::install(catalog).map_err(|_| "content catalog was installed more than once")?;
    let mut args = arguments.into_iter();
    let default_world = if cfg!(feature = "lifecycle-fixture") {
        "world-v25-fixture"
    } else {
        "world-v25"
    };
    match args.next().as_deref() {
        Some("model-preview") => {
            let usage =
                "usage: model-preview <model.glb> <output.png> [controls.json] [preview.json]";
            let model = args.next().ok_or(usage)?;
            let output = args.next().ok_or(usage)?;
            let controls = args.next();
            let options = args.next();
            if args.next().is_some() {
                return Err(usage.into());
            }
            preview::model::render(
                std::path::Path::new(&model),
                std::path::Path::new(&output),
                controls.as_deref().map(std::path::Path::new),
                options.as_deref().map(std::path::Path::new),
            )?;
        }
        Some("audio-insect-preview") => {
            let usage =
                "usage: audio-insect-preview <crickets|cicadas> <seconds> <output.wav> [seed]";
            let kind = args.next().ok_or(usage)?;
            let seconds: f32 = args.next().ok_or(usage)?.parse()?;
            let path = args.next().ok_or(usage)?;
            let seed = args
                .next()
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(1);
            if args.next().is_some() {
                return Err(usage.into());
            }
            audio::render_insect_preview(&kind, seconds, std::path::Path::new(&path), seed)?;
        }
        Some("audio-material-preview") => {
            let usage = "usage: audio-material-preview <water|dirt|leaf|stone|glass|metal|plastic|asphalt|roof|wood|split|custom> <seconds> <output.wav> [seed]";
            let profile = args.next().ok_or(usage)?;
            let seconds: f32 = args.next().ok_or(usage)?.parse()?;
            let path = args.next().ok_or(usage)?;
            let seed = args
                .next()
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(1);
            if args.next().is_some() {
                return Err(usage.into());
            }
            audio::render_material_preview(&profile, seconds, std::path::Path::new(&path), seed)?;
        }
        Some("audio-preview") => {
            let usage = "usage: audio-preview <rain|storm|wind|off> <seconds> <output.wav> [seed]";
            let preset = args
                .next()
                .and_then(|s| audio::Preset::parse(&s))
                .ok_or(usage)?;
            let seconds: f32 = args.next().ok_or(usage)?.parse()?;
            let path = args.next().ok_or(usage)?;
            let seed = args
                .next()
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(1);
            if args.next().is_some() {
                return Err(usage.into());
            }
            audio::render_preview(preset, seconds, std::path::Path::new(&path), seed)?;
        }
        Some("audio-file") => {
            let usage = "usage: audio-file <input.wav> [seconds]";
            let path = args.next().ok_or(usage)?;
            let seconds = args
                .next()
                .map(|s| s.parse::<f32>())
                .transpose()?
                .unwrap_or(10.0);
            if args.next().is_some() {
                return Err(usage.into());
            }
            audio::play_file(std::path::Path::new(&path), seconds)?;
        }
        Some("audio-play") => {
            let usage = "usage: audio-play <rain|storm|wind|off> [seconds]";
            let preset = args
                .next()
                .and_then(|s| audio::Preset::parse(&s))
                .ok_or(usage)?;
            let seconds = args
                .next()
                .map(|s| s.parse::<f32>())
                .transpose()?
                .unwrap_or(10.0);
            if args.next().is_some() {
                return Err(usage.into());
            }
            audio::play_preview(preset, seconds)?;
        }
        None | Some("local") => {
            let save_dir = args.next().unwrap_or_else(|| default_world.to_owned());
            if args.next().is_some() {
                return Err("usage: local [save-dir]".into());
            }
            let config_path = config::Config::default_path();
            let mut config = config::Config::load(&config_path);
            config.ensure_profile(&config_path)?;
            let (addr, server) = server::start_local_server_with_admin(
                0xB10C_6100,
                save_dir.into(),
                config.profile,
            )?;
            let client_result = client::run_client_with_admin(&addr.to_string());
            let server_result = server.stop();
            client_result?;
            server_result?;
        }
        Some("local-packages") => {
            let usage = "usage: local-packages <package-root> <save-dir>";
            let root = args.next().ok_or(usage)?;
            let save_dir = args.next().ok_or(usage)?;
            if args.next().is_some() {
                return Err(usage.into());
            }
            let config_path = config::Config::default_path();
            let mut config = config::Config::load(&config_path);
            config.ensure_profile(&config_path)?;
            let (addr, server) = server::start_local_server_with_packages(
                0xB10C_6100,
                save_dir.into(),
                config.profile,
                std::path::Path::new(&root),
            )?;
            let client_result = client::run_client_with_admin(&addr.to_string());
            let server_result = server.stop();
            client_result?;
            server_result?;
        }
        Some("server") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            let save_dir = args.next().unwrap_or_else(|| default_world.to_string());
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
        Some("server-packages") => {
            let usage =
                "usage: server-packages <package-root> <address> <save-dir> [max-clients: 1..=256]";
            let root = args.next().ok_or(usage)?;
            let addr = args.next().ok_or(usage)?;
            let save = args.next().ok_or(usage)?;
            let limit = args
                .next()
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(128);
            if args.next().is_some() {
                return Err(usage.into());
            }
            server::run_server_with_local_packages(
                &addr,
                0xB10C_6100,
                save.into(),
                limit,
                std::path::Path::new(&root),
            )?;
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
        Some("fire-preview") => {
            let path = args.next().unwrap_or_else(|| "fire-preview.png".to_owned());
            preview::render_fire_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
        }
        Some("visual-preview") => {
            let directory = args
                .next()
                .ok_or("usage: visual-preview <output-dir> <package-root> <state-key>")?;
            let root = args.next().ok_or("missing package root")?;
            let state = args.next().ok_or("missing state key")?;
            if args.next().is_some() {
                return Err("too many visual-preview arguments".into());
            }
            preview::render_visual_previews(
                std::path::Path::new(&directory),
                std::path::Path::new(&root),
                &state,
            )?;
        }
        Some("effect-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "effect-preview.png".to_owned());
            preview::render_effect_preview(std::path::Path::new(&path))?;
        }
        Some("ui-preview") => {
            let directory = args.next().unwrap_or_else(|| "ui-previews".to_string());
            if let Some(package_root) = args.next() {
                preview::render_package_ui_previews(
                    std::path::Path::new(&directory),
                    std::path::Path::new(&package_root),
                )?;
            } else {
                preview::render_ui_previews(std::path::Path::new(&directory))?;
            }
            println!("wrote UI previews to {directory}");
        }
        Some("egui-preview") => {
            let directory = args.next().unwrap_or_else(|| "egui-previews".to_string());
            let package_root = args.next();
            if args.next().is_some() {
                return Err("usage: egui-preview [output-dir] [package-root]".into());
            }
            if let Some(root) = package_root {
                preview::render_package_egui_previews(
                    std::path::Path::new(&directory),
                    std::path::Path::new(&root),
                )?;
            } else {
                preview::render_egui_previews(std::path::Path::new(&directory))?;
            }
            println!("wrote egui previews to {directory}");
        }
        Some("weather-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "weather-previews".to_string());
            if args.next().is_some() {
                return Err("usage: weather-preview [output-dir]".into());
            }
            preview::render_weather_previews(std::path::Path::new(&directory))?;
            println!("wrote weather previews to {directory}");
        }
        Some("daylight-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "daylight-previews".to_string());
            if args.next().is_some() {
                return Err("usage: daylight-preview [output-dir]".into());
            }
            preview::render_daylight_previews(std::path::Path::new(&directory))?;
            println!("wrote daylight previews to {directory}");
        }
        Some("sandbox-preview") => {
            let usage = "usage: sandbox-preview [output-dir] [all|workshop|factory|neon] [all|noon|sunset|night] [hero|characters] [idle|walk]";
            let directory = args
                .next()
                .unwrap_or_else(|| "sandbox-previews".to_string());
            let theme = args.next().unwrap_or_else(|| "all".to_string());
            let time = args.next().unwrap_or_else(|| "all".to_string());
            let view = args.next().unwrap_or_else(|| "hero".to_string());
            let pose = args.next().unwrap_or_else(|| "idle".to_string());
            if args.next().is_some() {
                return Err(usage.into());
            }
            preview::render_sandbox_previews(
                std::path::Path::new(&directory),
                &theme,
                &time,
                &view,
                &pose,
            )?;
            println!("wrote sandbox rendering fixtures to {directory}");
        }
        Some("calibration-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "calibration-previews".to_string());
            if args.next().is_some() {
                return Err("usage: calibration-preview [output-dir]".into());
            }
            preview::render_calibration_previews(std::path::Path::new(&directory))?;
            println!("wrote lighting calibration previews to {directory}");
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
        Some("mossbun-motion-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "mossbun-motion-previews".to_string());
            if args.next().is_some() {
                return Err("usage: mossbun-motion-preview [directory]".into());
            }
            preview::render_mossbun_motion_previews(std::path::Path::new(&path))?;
        }
        Some("block-preview") => {
            let usage = "usage: block-preview <state-key> <output.png> [package-root]";
            let key = args.next().ok_or(usage)?;
            let path = args.next().ok_or(usage)?;
            let _package_root = args.next();
            if args.next().is_some() {
                return Err(usage.into());
            }
            preview::render_block_preview(&key, std::path::Path::new(&path))?;
        }
        Some("creature-preview") => {
            let usage = "usage: creature-preview <entity-key> <output.png> [package-root] [r,g,b]";
            let key = args.next().ok_or(usage)?;
            let path = args.next().ok_or(usage)?;
            let _package_root = args.next();
            let tint = args.next().map(|value| parse_tint(&value)).transpose()?;
            if args.next().is_some() {
                return Err(usage.into());
            }
            preview::render_creature_preview(&key, std::path::Path::new(&path), tint)?;
        }
        Some("inventory-preview") => {
            let usage = "usage: inventory-preview <entity-key> <directory> [package-root]";
            let entity = args.next().ok_or(usage)?;
            let path = args.next().ok_or(usage)?;
            let _package_root = args.next();
            if args.next().is_some() {
                return Err(usage.into());
            }
            preview::render_inventory_previews(&entity, std::path::Path::new(&path))?;
        }
        Some("chest-preview") => {
            let path = args.next().unwrap_or_else(|| "chest-preview".to_owned());
            if args.next().is_some() {
                return Err("usage: chest-preview [directory]".into());
            }
            preview::render_chest_previews(std::path::Path::new(&path))?;
        }
        Some("hopper-preview") => {
            let path = args.next().unwrap_or_else(|| "hopper-preview".to_string());
            if args.next().is_some() {
                return Err("usage: hopper-preview [directory]".into());
            }
            preview::render_hopper_previews(std::path::Path::new(&path))?;
        }
        Some("kiln-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "kiln-preview.png".to_string());
            if args.next().is_some() {
                return Err("usage: kiln-preview [output.png]".into());
            }
            preview::render_kiln_preview(std::path::Path::new(&path))?;
        }
        Some("mossbun-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "mossbun-preview.png".to_string());
            if args.next().is_some() {
                return Err("usage: mossbun-preview [output.png]".into());
            }
            preview::render_mossbun_preview(std::path::Path::new(&path))?;
        }
        Some("third-person-preview") => {
            let directory = args.next().unwrap_or_else(|| "third-person-preview".into());
            if args.next().is_some() {
                return Err("usage: third-person-preview [directory]".into());
            }
            preview::render_third_person_previews(std::path::Path::new(&directory))?;
        }
        Some("first-person-preview") => {
            let path = args.next().unwrap_or_else(|| "first-person-preview".into());
            if args.next().is_some() {
                return Err("usage: first-person-preview [directory]".into());
            }
            preview::render_first_person_previews(std::path::Path::new(&path))?;
        }
        Some("character-gameplay-preview") => {
            let directory = args
                .next()
                .unwrap_or_else(|| "character-gameplay-preview".into());
            if args.next().is_some() {
                return Err("usage: character-gameplay-preview [directory]".into());
            }
            preview::render_gameplay_animation_previews(std::path::Path::new(&directory))?;
        }
        Some("character-style-preview") => {
            let path = args.next().unwrap_or_else(|| "character-styles.png".into());
            if args.next().is_some() {
                return Err("usage: character-style-preview [output.png]".into());
            }
            preview::render_character_styles(std::path::Path::new(&path))?;
        }
        Some("character-motion-preview") => {
            let directory = args.next().unwrap_or_else(|| "character-motion".into());
            if args.next().is_some() {
                return Err("usage: character-motion-preview [directory]".into());
            }
            preview::render_character_motion(std::path::Path::new(&directory))?;
        }
        Some("character-preview") => {
            let path = args
                .next()
                .unwrap_or_else(|| "character-preview.png".into());
            let clip = args.next().unwrap_or_else(|| "idle".into());
            let time = args
                .next()
                .map(|s| s.parse::<f32>())
                .transpose()?
                .unwrap_or(0.0);
            let hair = args
                .next()
                .map(|s| s.parse::<u8>())
                .transpose()?
                .unwrap_or(1);
            if args.next().is_some() {
                return Err("usage: character-preview [output.png] [idle|walk|run|crouch|tool_use_left|tool_use_right] [seconds] [hair-id]".into());
            }
            preview::render_character_preview(std::path::Path::new(&path), &clip, time, hair)?;
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
        Some("character-perf") => {
            let frames = args
                .next()
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(300);
            let actors = args
                .next()
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(128);
            let hair = match args.next().as_deref() {
                Some("default") => None,
                Some(value) => Some(value.parse::<u8>()?),
                None => Some(1),
            };
            if args.next().is_some() {
                return Err(
                    "usage: character-perf [measured-frames] [actors] [hair-id|default]".into(),
                );
            }
            preview::run_character_benchmark(frames, actors, hair)?;
        }
        Some("lod-preview") => {
            let directory = args.next().unwrap_or_else(|| "lod-previews".into());
            let horizon = args
                .next()
                .map(|v| v.parse::<u16>())
                .transpose()?
                .unwrap_or(512);
            if args.next().is_some() {
                return Err("usage: lod-preview [output-dir] [512|1024]".into());
            }
            preview::render_lod_previews(std::path::Path::new(&directory), horizon)?;
        }
        Some("lod-perf") => {
            let frames = args
                .next()
                .map(|v| v.parse::<usize>())
                .transpose()?
                .unwrap_or(300);
            let radius = args
                .next()
                .map(|v| v.parse::<u8>())
                .transpose()?
                .unwrap_or(6);
            let horizon = args
                .next()
                .map(|v| v.parse::<u16>())
                .transpose()?
                .unwrap_or(512);
            let bounced = match args.next().as_deref() {
                None | Some("voxel") => false,
                Some("bounced") => true,
                _ => {
                    return Err(
                        "usage: lod-perf [frames] [near-radius] [0|512|1024] [voxel|bounced]"
                            .into(),
                    );
                }
            };
            if args.next().is_some() {
                return Err(
                    "usage: lod-perf [frames] [near-radius] [0|512|1024] [voxel|bounced]".into(),
                );
            }
            preview::run_lod_benchmark(frames, radius, horizon, bounced)?;
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

fn parse_tint(value: &str) -> Result<[f32; 3], Box<dyn std::error::Error>> {
    let channels = value
        .split(',')
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()?;
    let color: [f32; 3] = channels
        .try_into()
        .map_err(|_| "tint must have exactly three comma-separated channels")?;
    if color.iter().any(|channel| !(0.0..=1.0).contains(channel)) {
        return Err("tint channels must be finite values from 0 to 1".into());
    }
    Ok(color)
}
