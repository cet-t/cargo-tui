use crate::{app::{App, Event}, ui, workspace::WorkspaceInfo};
use anyhow::Result;
use crossterm::{
    event::{self, Event as CEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::sync::mpsc;

pub async fn run(info: WorkspaceInfo) -> Result<()> {
    // ── ターミナル初期化 ──────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // ── イベントチャネル ──────────────────────────────────────
    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

    // キーボード読み取りタスク
    let key_tx = tx.clone();
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(16)).unwrap_or(false) {
                if let Ok(CEvent::Key(key)) = event::read() {
                    if key.kind == KeyEventKind::Press {
                        if key_tx.send(Event::Key(key)).is_err() {
                            break;
                        }
                    }
                }
            }
            // Tick
            if key_tx.send(Event::Tick).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // ── アプリ ───────────────────────────────────────────────
    let mut app = App::new(info, tx);

    // インストール済みクレート先頭の詳細をプリフェッチ
    if let Some(dep) = app.pkg_deps.first().cloned() {
        app.fetch_detail(dep.name, dep.version, false);
    }

    // ── メインループ ─────────────────────────────────────────
    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if let Some(event) = rx.recv().await {
            app.handle(event);
        }

        if app.quit {
            break;
        }
    }

    // ── ターミナル復元 ────────────────────────────────────────
    terminal.show_cursor()?;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}
