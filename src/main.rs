mod client;
mod config;
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
            let (addr, _server) = server::start_local_server(0xB10C_6100, "world".into())?;
            client::run_client(&addr.to_string())?;
        }
        Some("server") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            let save_dir = args.next().unwrap_or_else(|| "world".to_string());
            server::run_server(&addr, 0xB10C_6100, save_dir.into())?;
        }
        Some("client") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            client::run_client(&addr)?;
        }
        Some("preview") => {
            let path = args.next().unwrap_or_else(|| "preview.png".to_string());
            preview::render_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
        }
        Some("ui-preview") => {
            let directory = args.next().unwrap_or_else(|| "ui-previews".to_string());
            preview::render_ui_previews(std::path::Path::new(&directory))?;
            println!("wrote UI previews to {directory}");
        }
        Some(other) => {
            return Err(format!("unknown command {other:?}; use `server [address] [save-dir]`, `client [address]`, `preview [output.png]`, or `ui-preview [output-dir]`").into());
        }
    }
    Ok(())
}
