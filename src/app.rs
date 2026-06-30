use crate::{
    config::KeyConfig,
    crates_io::{CrateDetail, CrateInfo},
    runner::RunnerEvent,
    workspace::{Dep, WorkspaceInfo},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

// ── Tab / section ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    BuildRun = 0,
    Package  = 1,
    Test     = 2,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PkgSection {
    Installed,
    Search,
}

// ── Command definitions ───────────────────────────────────────

#[derive(Clone)]
pub struct Cmd {
    pub label:   &'static str,
    pub args:    &'static [&'static str],
    /// If set, prompt the user for additional input before running.
    pub prompt:  Option<&'static str>,
    pub section: &'static str,
}

pub const BUILD_RUN_CMDS: &[Cmd] = &[
    Cmd { label: "build",             args: &["build"],              prompt: None,           section: "BUILD" },
    Cmd { label: "build --release",   args: &["build", "--release"], prompt: None,           section: "BUILD" },
    Cmd { label: "build --target …",  args: &["build", "--target"],  prompt: Some("target"), section: "BUILD" },
    Cmd { label: "run",               args: &["run"],                prompt: None,           section: "RUN"   },
    Cmd { label: "run --release",     args: &["run", "--release"],   prompt: None,           section: "RUN"   },
    Cmd { label: "run -- <args>",     args: &["run", "--"],          prompt: Some("args"),   section: "RUN"   },
    Cmd { label: "fmt",               args: &["fmt"],                prompt: None,           section: "TOOLS" },
    Cmd { label: "clippy",            args: &["clippy"],             prompt: None,           section: "TOOLS" },
    Cmd { label: "check",             args: &["check"],              prompt: None,           section: "TOOLS" },
    Cmd { label: "clean",             args: &["clean"],              prompt: None,           section: "TOOLS" },
    Cmd { label: "doc --open",        args: &["doc", "--open"],      prompt: None,           section: "TOOLS" },
    Cmd { label: "publish --dry-run", args: &["publish","--dry-run"],prompt: None,           section: "TOOLS" },
    Cmd { label: "publish",           args: &["publish"],            prompt: None,           section: "TOOLS" },
];

pub const TEST_CMDS: &[Cmd] = &[
    Cmd { label: "test (all)",     args: &["test"],            prompt: None,           section: "TEST" },
    Cmd { label: "test --release", args: &["test","--release"],prompt: None,           section: "TEST" },
    Cmd { label: "test <filter>",  args: &["test"],            prompt: Some("filter"), section: "TEST" },
    Cmd { label: "test --no-run",  args: &["test","--no-run"], prompt: None,           section: "TEST" },
];

// ── Events ────────────────────────────────────────────────────

pub enum Event {
    Key(KeyEvent),
    Runner(RunnerEvent),
    SearchResult(Vec<CrateInfo>),
    DetailResult(bool /* is_search */, CrateDetail),
    Tick,
}

// ── Test result ───────────────────────────────────────────────

#[derive(Clone)]
pub struct TestResult {
    pub name: String,
    pub ok:   bool,
}

// ── Application state ─────────────────────────────────────────

pub struct App {
    pub root:    PathBuf,
    pub ws_name: String,
    pub quit:    bool,

    // tabs
    pub tab: Tab,

    // Build/Run + Test shared output state
    pub br_sel:       usize,
    pub test_sel:     usize,
    pub output:       Vec<String>,
    pub running:      bool,
    pub last_args:    Option<Vec<String>>,
    pub test_results: Vec<TestResult>,
    pub kill_tx:      Option<oneshot::Sender<()>>,

    // Package tab
    pub pkg_section:     PkgSection,
    pub pkg_deps:        Vec<Dep>,
    pub pkg_sel_inst:    usize,
    pub pkg_search_mode: bool,
    pub pkg_query:       String,
    pub pkg_results:     Vec<CrateInfo>,
    pub pkg_sel_search:  usize,
    pub pkg_loading:     bool,
    pub pkg_detail_inst: Option<CrateDetail>,
    pub pkg_detail_srch: Option<CrateDetail>,

    pub event_tx: mpsc::UnboundedSender<Event>,
    pub key: KeyConfig,
}

impl App {
    pub fn new(info: WorkspaceInfo, key: KeyConfig, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            root:    info.root,
            ws_name: info.name,
            quit:    false,
            tab:     Tab::BuildRun,
            br_sel:       0,
            test_sel:     0,
            output:       vec![],
            running:      false,
            last_args:    None,
            test_results: vec![],
            kill_tx:      None,
            pkg_section:     PkgSection::Installed,
            pkg_deps:        info.deps,
            pkg_sel_inst:    0,
            pkg_search_mode: false,
            pkg_query:       String::new(),
            pkg_results:     vec![],
            pkg_sel_search:  0,
            pkg_loading:     false,
            pkg_detail_inst: None,
            pkg_detail_srch: None,
            event_tx,
            key,
        }
    }

    // ── Run cargo command ─────────────────────────────────────

    pub fn run_cargo(&mut self, args: Vec<String>) {
        if let Some(tx) = self.kill_tx.take() {
            let _ = tx.send(());
        }
        self.output = vec![format!("$ cargo {}", args.join(" "))];
        self.running   = true;
        self.last_args = Some(args.clone());

        let (runner_tx, mut runner_rx) = mpsc::unbounded_channel::<RunnerEvent>();
        let kill_tx = crate::runner::spawn(args, self.root.clone(), runner_tx);
        self.kill_tx = Some(kill_tx);

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = runner_rx.recv().await {
                if event_tx.send(Event::Runner(ev)).is_err() {
                    break;
                }
            }
        });
    }

    pub fn kill(&mut self) {
        if let Some(tx) = self.kill_tx.take() {
            let _ = tx.send(());
        }
    }

    // ── Fetch crate detail from crates.io ────────────────────

    pub fn fetch_detail(&self, name: String, version: String, is_search: bool) {
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            if let Ok(detail) = crate::crates_io::get_detail(&name, &version).await {
                let _ = tx.send(Event::DetailResult(is_search, detail));
            }
        });
    }

    /// Trigger a debounce-free crates.io search for the current query.
    pub fn trigger_search(&mut self) {
        if self.pkg_query.is_empty() {
            self.pkg_results    = vec![];
            self.pkg_loading    = false;
            self.pkg_detail_srch = None;
            return;
        }
        self.pkg_loading = true;
        let query = self.pkg_query.clone();
        let tx    = self.event_tx.clone();
        tokio::spawn(async move {
            if let Ok(results) = crate::crates_io::search(&query, 20).await {
                let _ = tx.send(Event::SearchResult(results));
            }
        });
    }

    // ── Event handler ─────────────────────────────────────────

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Tick => {}

            Event::Runner(RunnerEvent::Line(line)) => {
                // Parse test results from cargo test output
                if self.tab == Tab::Test {
                    if let Some(name) = line
                        .strip_prefix("test ")
                        .and_then(|l| l.strip_suffix(" ... ok"))
                    {
                        self.test_results.push(TestResult { name: name.to_string(), ok: true });
                    } else if let Some(name) = line
                        .strip_prefix("test ")
                        .and_then(|l| l.strip_suffix(" ... FAILED"))
                    {
                        self.test_results.push(TestResult { name: name.to_string(), ok: false });
                    }
                }
                self.output.push(line);
            }

            Event::Runner(RunnerEvent::Exit(code)) => {
                self.running  = false;
                self.kill_tx  = None;
                let icon = if code == 0 { "✓" } else { "✗" };
                self.output.push(String::new());
                self.output.push(format!("  {} exit code: {}", icon, code));
            }

            Event::SearchResult(results) => {
                self.pkg_loading     = false;
                self.pkg_sel_search  = 0;
                self.pkg_detail_srch = None;
                if let Some(first) = results.first() {
                    self.fetch_detail(first.name.clone(), first.version.clone(), true);
                }
                self.pkg_results = results;
            }

            Event::DetailResult(is_search, detail) => {
                if is_search {
                    self.pkg_detail_srch = Some(detail);
                } else {
                    self.pkg_detail_inst = Some(detail);
                }
            }

            Event::Key(key) => self.handle_key(key),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-C always kills or quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.running { self.kill(); } else { self.quit = true; }
            return;
        }

        // In search mode: text input + arrow navigation
        if self.pkg_search_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => { self.pkg_search_mode = false; }
                KeyCode::Backspace => { self.pkg_query.pop(); self.trigger_search(); }
                KeyCode::Up        => self.move_search_sel(-1),
                KeyCode::Down      => self.move_search_sel(1),
                KeyCode::Char(c)   => {
                    self.pkg_query.push(c);
                    self.pkg_sel_search = 0;
                    self.trigger_search();
                }
                _ => {}
            }
            return;
        }

        let k = self.key.clone();

        // Quit (Esc always works as a fallback)
        if k.quit.matches(&key) || key.code == KeyCode::Esc {
            self.quit = true;
            return;
        }

        // Tab switching
        if k.tab_1.matches(&key)    { self.switch_tab(Tab::BuildRun); return; }
        if k.tab_2.matches(&key)    { self.switch_tab(Tab::Package);  return; }
        if k.tab_3.matches(&key)    { self.switch_tab(Tab::Test);     return; }
        if k.tab_next.matches(&key) { self.next_tab(); return; }
        if k.tab_prev.matches(&key) { self.prev_tab(); return; }

        // Navigation (arrow keys always work)
        if k.down.matches(&key) || key.code == KeyCode::Down { self.move_sel(1);  return; }
        if k.up.matches(&key)   || key.code == KeyCode::Up   { self.move_sel(-1); return; }

        // Run / re-run / kill
        if k.run.matches(&key) { self.do_run(); return; }
        if k.rerun.matches(&key) {
            if let Some(args) = self.last_args.clone() {
                self.output.clear();
                self.run_cargo(args);
            }
            return;
        }
        if k.kill.matches(&key) { self.kill(); return; }

        // Package tab actions
        if self.tab == Tab::Package {
            if k.pkg_toggle.matches(&key) {
                self.pkg_section = match self.pkg_section {
                    PkgSection::Installed => PkgSection::Search,
                    PkgSection::Search    => PkgSection::Installed,
                };
                return;
            }
            if k.pkg_search.matches(&key) {
                self.pkg_search_mode = true;
                self.pkg_section     = PkgSection::Search;
                return;
            }
            if k.pkg_remove.matches(&key) && self.pkg_section == PkgSection::Installed {
                if let Some(dep) = self.pkg_deps.get(self.pkg_sel_inst).cloned() {
                    self.run_cargo(vec!["remove".into(), dep.name]);
                }
                return;
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.output.clear();
        self.test_results.clear();
    }

    fn next_tab(&mut self) {
        self.switch_tab(match self.tab {
            Tab::BuildRun => Tab::Package,
            Tab::Package  => Tab::Test,
            Tab::Test     => Tab::BuildRun,
        });
    }

    fn prev_tab(&mut self) {
        self.switch_tab(match self.tab {
            Tab::BuildRun => Tab::Test,
            Tab::Package  => Tab::BuildRun,
            Tab::Test     => Tab::Package,
        });
    }

    fn move_sel(&mut self, delta: i32) {
        match self.tab {
            Tab::BuildRun => {
                self.br_sel = clamp_move(self.br_sel, delta, BUILD_RUN_CMDS.len());
            }
            Tab::Test => {
                self.test_sel = clamp_move(self.test_sel, delta, TEST_CMDS.len());
            }
            Tab::Package => match self.pkg_section {
                PkgSection::Installed => {
                    let n    = self.pkg_deps.len();
                    let prev = self.pkg_sel_inst;
                    self.pkg_sel_inst = clamp_move(self.pkg_sel_inst, delta, n);
                    if self.pkg_sel_inst != prev {
                        self.pkg_detail_inst = None;
                        if let Some(dep) = self.pkg_deps.get(self.pkg_sel_inst).cloned() {
                            self.fetch_detail(dep.name, dep.version, false);
                        }
                    }
                }
                PkgSection::Search => {
                    self.move_search_sel(delta);
                }
            },
        }
    }

    fn move_search_sel(&mut self, delta: i32) {
        let n = self.pkg_results.len();
        if n == 0 { return; }
        let prev = self.pkg_sel_search;
        self.pkg_sel_search = clamp_move(self.pkg_sel_search, delta, n);
        if self.pkg_sel_search != prev {
            self.pkg_detail_srch = None;
            if let Some(r) = self.pkg_results.get(self.pkg_sel_search).cloned() {
                self.fetch_detail(r.name, r.version, true);
            }
        }
    }

    fn do_run(&mut self) {
        match self.tab {
            Tab::BuildRun => {
                let cmd = &BUILD_RUN_CMDS[self.br_sel];
                if cmd.prompt.is_none() {
                    let args = cmd.args.iter().map(|s| s.to_string()).collect();
                    self.output.clear();
                    self.run_cargo(args);
                }
            }
            Tab::Test => {
                let cmd = &TEST_CMDS[self.test_sel];
                if cmd.prompt.is_none() {
                    let args = cmd.args.iter().map(|s| s.to_string()).collect();
                    self.output.clear();
                    self.run_cargo(args);
                }
            }
            Tab::Package => {
                if self.pkg_section == PkgSection::Search {
                    if let Some(r) = self.pkg_results.get(self.pkg_sel_search).cloned() {
                        self.run_cargo(vec!["add".into(), r.name]);
                    }
                }
            }
        }
    }
}

fn clamp_move(cur: usize, delta: i32, len: usize) -> usize {
    if len == 0 { return 0; }
    (cur as i32 + delta).max(0).min(len as i32 - 1) as usize
}
