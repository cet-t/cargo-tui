# cargo-tui

A [gitui](https://github.com/extrawurst/gitui)-style terminal UI for Cargo.
Build, run, test, and manage dependencies of your Rust projects from a fast,
keyboard- and mouse-driven TUI.

## Features

- **Build / Run** — `build`, `run`, `fmt`, `clippy`, `check`, `clean`, `doc`,
  `publish`, and more. Run targets are detected automatically via
  `cargo metadata`:
  - library-only crates show a clear "nothing to run" note instead of a raw
    cargo error,
  - a single binary runs directly,
  - multiple binaries and `examples/*.rs` each get their own entry
    (`cargo run --bin <name>` / `cargo run --example <name>`).
- **Crate** — manage dependencies per section (`[dependencies]`,
  `[dev-dependencies]`, `[build-dependencies]`). Search crates.io, add to the
  section of your choice, and remove — the installed list refreshes in real
  time.
- **Test** — run the whole suite or a subset and see per-test pass/fail results.
- **Scrollable output** — focus the Output/Description pane and scroll it with
  `hjkl`.
- **Mouse support** — click tabs and list rows, and scroll with the wheel.
- **Configurable keys** — every binding can be changed from a TOML config file.

## Installation

Install globally with your preferred Node package manager:

```sh
bun add -g cargo-tui
# or
npm install -g cargo-tui
# or
pnpm add -g cargo-tui
```

The correct prebuilt binary for your platform (Windows / Linux / macOS, x64 and
arm64) is fetched automatically as an optional dependency.

### From source

```sh
git clone https://github.com/cet-t/cargo-tui
cd cargo-tui
cargo build --release
# binary at target/release/cargo-tui
```

## Usage

Run it inside (or above) a Cargo project:

```sh
cargo-tui
```

Options:

| Flag | Description |
| --- | --- |
| `--manifest-path <PATH>` | Path to `Cargo.toml` (or its parent directory). |
| `--config <FILE>` | Path to a config file (see below). |
| `-h`, `--help` | Print help. |

## Key bindings

Arrow keys always work in addition to the configured keys.

| Key | Action |
| --- | --- |
| `1` / `2` / `3` | Switch to Build/Run / Crate / Test |
| `]` / `[` | Next / previous tab |
| `j` / `k` | Move selection down / up |
| `Enter` | Run the selected command / add the selected crate |
| `r` | Re-run the last command |
| `K` | Kill the running process |
| `l` | Focus the Output/Description pane; while focused, `hjkl` scroll |
| `h` | Scroll left; at the left edge, return focus to the list |
| `s` | Search crates.io (Crate tab) |
| `Enter` | Run the search (in search input) |
| `p` | Cycle the target section for adding (dependencies → dev → build) |
| `d` | Remove the selected installed crate |
| `Tab` | Toggle Installed / Search (Crate tab) |
| `q` / `Esc` | Quit |
| `Ctrl+C` | Kill the running process, or quit |

### Mouse

- Click a **tab** to switch to it.
- Click a **list row** to select it.
- Click the **right pane** to focus it for scrolling.
- **Wheel** scrolls the right pane when the cursor is over it, otherwise moves
  the selection.

## Configuration

Keys are read from a TOML file. Default location:

- **Windows:** `%APPDATA%\cargo-tui\config.toml`
- **Linux / macOS:** `~/.config/cargo-tui/config.toml`

Or pass an explicit path with `--config`. Every key is optional and falls back
to the default shown below. Values accept plain characters (`"q"`), named keys
(`"enter"`, `"esc"`, `"tab"`, `"up"`, `"f5"`, …) and modifiers (`"ctrl+c"`).

```toml
[keys]
quit        = "q"
tab_1       = "1"
tab_2       = "2"
tab_3       = "3"
tab_next    = "]"
tab_prev    = "["
down        = "j"
up          = "k"
focus_right = "l"
focus_left  = "h"
run         = "enter"
rerun       = "r"
kill        = "K"
pkg_search  = "s"
pkg_remove  = "d"
pkg_toggle  = "tab"
pkg_profile = "p"
```

## License

MIT
