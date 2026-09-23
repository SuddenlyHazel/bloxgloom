mod client;
mod preview;
mod protocol;
mod render;
mod server;
mod storage;
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
        Some("server") => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            let save_dir = args.next().unwrap_or_else(|| "world".to_string());
            server::run_server(&addr, 0xB10C_6100, save_dir.into())?;
        }
        Some("client") | None => {
            let addr = args.next().unwrap_or_else(|| "127.0.0.1:4000".to_string());
            client::run_client(&addr)?;
        }
        Some("preview") => {
            let path = args.next().unwrap_or_else(|| "preview.png".to_string());
            preview::render_preview(std::path::Path::new(&path))?;
            println!("wrote {path}");
        }
        Some(other) => {
            return Err(format!("unknown command {other:?}; use `server [address] [save-dir]`, `client [address]`, or `preview [output.png]`").into());
        }
    }
    Ok(())
}
