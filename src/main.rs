mod capture;
mod cli;
mod encode;
mod frames;
mod gui;
mod mcp;
mod recording;
mod safety;
mod sessions;

fn main() {
    capture::init_dpi();
    if let Err(e) = cli::run() {
        eprintln!(
            "{}",
            serde_json::json!({"ok":false,"error":format!("{e:#}")})
        );
        std::process::exit(1);
    }
}
