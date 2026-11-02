mod capture;
mod cli;
mod encode;
mod encode_metrics;
mod file_guard;
mod frames;
mod gui;
mod hash_writer;
mod identity;
mod integration;
mod json_io;
mod mcp;
mod recording;
mod review;
mod review_cache;
mod runtime;
mod safety;
mod series;
mod sessions;
mod storage;
mod timeline;
mod transaction;
mod worker;

fn main() {
    runtime::install();
    capture::init_dpi();
    if let Err(e) = runtime::guard("application", cli::run) {
        let failure = runtime::report("application", &e);
        {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr().lock(),
                "{}",
                serde_json::json!({"ok":false,"error":failure.message,"failure":failure})
            );
        }
        std::process::exit(1);
    }
}
