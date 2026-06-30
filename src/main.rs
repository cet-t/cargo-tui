mod app;
mod config;
mod crates_io;
mod error;
mod runner;
mod tui;
mod ui;
mod workspace;

use clap::Parser;
use error::Error;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cargo-tui", about = "A gitui-style TUI for cargo")]
struct Args {
    /// Path to Cargo.toml (or its parent directory)
    #[arg(long, value_name = "PATH")]
    manifest_path: Option<PathBuf>,

    /// Path to a config file (default: %APPDATA%\cargo-tui\config.toml on Windows,
    /// ~/.config/cargo-tui/config.toml elsewhere)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let root = match args.manifest_path {
        Some(p) => {
            if p.ends_with("Cargo.toml") {
                p.parent().unwrap().to_path_buf()
            } else {
                p
            }
        }
        None => workspace::find_root(std::env::current_dir()?)
            .ok_or(Error::NoCargoToml)?,
    };

    let cfg = config::load(args.config.as_deref());
    let info = workspace::load(&root)?;
    tui::run(info, cfg.keys).await?;
    Ok(())
}
