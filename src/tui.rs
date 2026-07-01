use crate::{app::{App, Event}, config::KeyConfig, ui, workspace::WorkspaceInfo};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::sync::mpsc;

pub async fn run(info: WorkspaceInfo, key: KeyConfig) -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // Event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

    // Keyboard reader task
    let key_tx = tx.clone();
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(16)).unwrap_or(false) {
                match event::read() {
                    Ok(CEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                        if key_tx.send(Event::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(CEvent::Mouse(m)) => {
                        if key_tx.send(Event::Mouse(m)).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            // Tick
            if key_tx.send(Event::Tick).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // App
    let mut app = App::new(info, key, tx);

    // Pre-fetch detail for the first installed crate
    if let Some(dep) = app.pkg_deps.first().cloned() {
        app.fetch_detail(dep.name, dep.version, false);
    }

    // Main loop
    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if let Some(event) = rx.recv().await {
            app.handle(event);
        }

        if app.quit {
            break;
        }
    }

    // Terminal restore
    terminal.show_cursor()?;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    Ok(())
}
