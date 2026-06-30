mod app;
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
#[command(name = "cargo-tui", about = "gitui 風の cargo TUI")]
struct Args {
    /// Cargo.toml のパスを直接指定
    #[arg(long, value_name = "PATH")]
    manifest_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `cargo tui` として呼ばれると argv[1] == "tui" になるので除去
    let mut raw: Vec<_> = std::env::args_os().collect();
    if raw.get(1).map(|a| a == "tui").unwrap_or(false) {
        raw.remove(1);
    }
    let args = Args::parse_from(raw);

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

    let info = workspace::load(&root)?;
    tui::run(info).await?;
    Ok(())
}
