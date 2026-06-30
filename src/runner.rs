use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

pub enum RunnerEvent {
    Line(String),
    Exit(i32),
}

/// cargo コマンドを非同期実行。出力は tx に送信される。
/// 返値の sender に () を送ると強制終了。
pub fn spawn(
    args: Vec<String>,
    cwd: PathBuf,
    tx: mpsc::UnboundedSender<RunnerEvent>,
) -> tokio::sync::oneshot::Sender<()> {
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut child = match Command::new("cargo")
            .args(&args)
            .current_dir(&cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(RunnerEvent::Line(format!("Error: {}", e)));
                let _ = tx.send(RunnerEvent::Exit(1));
                return;
            }
        };

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let stderr = BufReader::new(child.stderr.take().unwrap());

        let tx2 = tx.clone();
        let tx3 = tx.clone();

        let h1 = tokio::spawn(async move {
            let mut lines = stdout.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx2.send(RunnerEvent::Line(strip_ansi(&line))).is_err() {
                    break;
                }
            }
        });

        let h2 = tokio::spawn(async move {
            let mut lines = stderr.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx3.send(RunnerEvent::Line(strip_ansi(&line))).is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = kill_rx => {
                let _ = child.kill().await;
                let _ = tx.send(RunnerEvent::Line("[中断]".to_string()));
                let _ = tx.send(RunnerEvent::Exit(130));
            }
            status = child.wait() => {
                let _ = h1.await;
                let _ = h2.await;
                let code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                let _ = tx.send(RunnerEvent::Exit(code));
            }
        }
    });

    kill_tx
}

/// ANSI エスケープシーケンスを除去
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
