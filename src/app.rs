use crate::{
    config::KeyConfig,
    crates_io::{CrateDetail, CrateInfo},
    runner::RunnerEvent,
    workspace::{Dep, DepKind, RunKind, RunTarget, WorkspaceInfo},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::Cell;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

// ── Tab / section ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    BuildRun = 0,
    Crate    = 1,
    Test     = 2,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PkgSection {
    Installed,
    Search,
}

/// Which pane currently has keyboard focus. The right pane (Output on the
/// Build/Run and Test tabs, Description on the Crate tab) can be focused to
/// scroll through its content with hjkl.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    Left,
    Right,
}

// ── Command definitions ───────────────────────────────────────

/// Sentinel first-arg used for the placeholder shown when a project has no
/// runnable binary target. Recognized specially in `do_run`.
pub const NO_BIN_SENTINEL: &str = "__nobin__";

#[derive(Clone)]
pub struct Cmd {
    pub label:   String,
    pub args:    Vec<String>,
    /// If set, prompt the user for additional input before running.
    pub prompt:  Option<String>,
    pub section: String,
}

impl Cmd {
    fn new(label: &str, args: &[&str], prompt: Option<&str>, section: &str) -> Self {
        Cmd {
            label:   label.to_string(),
            args:    args.iter().map(|s| s.to_string()).collect(),
            prompt:  prompt.map(|s| s.to_string()),
            section: section.to_string(),
        }
    }
}

/// Build the Build/Run command list, resolving `run` entries against the
/// runnable targets (bins and examples) discovered in the workspace so
/// `cargo run` never fails with "a bin target must be available" or an
/// ambiguous-target error.
pub fn build_run_cmds(targets: &[RunTarget]) -> Vec<Cmd> {
    let bins: Vec<&RunTarget>     = targets.iter().filter(|t| t.kind == RunKind::Bin).collect();
    let examples: Vec<&RunTarget> = targets.iter().filter(|t| t.kind == RunKind::Example).collect();

    let mut cmds = vec![
        Cmd::new("build",            &["build"],              None,           "BUILD"),
        Cmd::new("build --release",  &["build", "--release"], None,           "BUILD"),
        Cmd::new("build --target …", &["build", "--target"],  Some("target"), "BUILD"),
    ];

    // ── RUN section (binaries) ───────────────────────────────────
    match bins.as_slice() {
        // No bins at all. Show a placeholder only when there is also nothing
        // else to run; otherwise the EXAMPLES section carries the runnables.
        [] if examples.is_empty() => cmds.push(Cmd::new(
            "run (no binary target)",
            &[NO_BIN_SENTINEL],
            None,
            "RUN",
        )),
        [] => {}
        // Exactly one binary: plain run/run --release, targeting it explicitly.
        [b] => {
            cmds.push(Cmd::new("run",           &["run", "--bin", &b.name],              None, "RUN"));
            cmds.push(Cmd::new("run --release", &["run", "--release", "--bin", &b.name], None, "RUN"));
        }
        // Multiple binaries: one row per binary so the user picks which to run.
        many => {
            for b in many {
                cmds.push(Cmd::new(
                    &format!("run {}", b.name),
                    &["run", "-p", &b.package, "--bin", &b.name],
                    None,
                    "RUN",
                ));
            }
            for b in many {
                cmds.push(Cmd::new(
                    &format!("run --release {}", b.name),
                    &["run", "--release", "-p", &b.package, "--bin", &b.name],
                    None,
                    "RUN",
                ));
            }
        }
    }

    // ── EXAMPLES section (examples/*.rs) ─────────────────────────
    for e in &examples {
        cmds.push(Cmd::new(
            &format!("run --example {}", e.name),
            &["run", "-p", &e.package, "--example", &e.name],
            None,
            "EXAMPLES",
        ));
    }
    for e in &examples {
        cmds.push(Cmd::new(
            &format!("run --release --example {}", e.name),
            &["run", "--release", "-p", &e.package, "--example", &e.name],
            None,
            "EXAMPLES",
        ));
    }

    cmds.extend([
        Cmd::new("fmt",               &["fmt"],                 None, "TOOLS"),
        Cmd::new("clippy",            &["clippy"],              None, "TOOLS"),
        Cmd::new("check",             &["check"],               None, "TOOLS"),
        Cmd::new("clean",             &["clean"],               None, "TOOLS"),
        Cmd::new("doc --open",        &["doc", "--open"],       None, "TOOLS"),
        Cmd::new("publish --dry-run", &["publish", "--dry-run"],None, "TOOLS"),
        Cmd::new("publish",           &["publish"],             None, "TOOLS"),
    ]);

    cmds
}

pub fn test_cmds() -> Vec<Cmd> {
    vec![
        Cmd::new("test (all)",     &["test"],             None,           "TEST"),
        Cmd::new("test --release", &["test", "--release"],None,           "TEST"),
        Cmd::new("test <filter>",  &["test"],             Some("filter"), "TEST"),
        Cmd::new("test --no-run",  &["test", "--no-run"], None,           "TEST"),
    ]
}

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

    // Focus + right-pane scrolling
    pub focus:    Focus,
    pub v_scroll: u16,
    pub h_scroll: u16,
    /// Right-pane viewport height / total content lines, updated during render
    /// so scrolling can be clamped accurately.
    pub right_view_lines:    Cell<u16>,
    pub right_content_lines: Cell<u16>,

    // Command lists (Build/Run resolves against detected bin targets)
    pub build_run_cmds: Vec<Cmd>,
    pub test_cmds:      Vec<Cmd>,

    // Build/Run + Test shared output state
    pub br_sel:       usize,
    pub test_sel:     usize,
    pub output:       Vec<String>,
    pub running:      bool,
    pub last_args:    Option<Vec<String>>,
    pub test_results: Vec<TestResult>,
    pub kill_tx:      Option<oneshot::Sender<()>>,

    // Crate tab
    pub pkg_section:     PkgSection,
    pub pkg_deps:        Vec<Dep>,
    pub pkg_sel_inst:    usize,
    /// Target dependency section used when adding a crate from search.
    pub pkg_add_kind:    DepKind,
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
            focus:    Focus::Left,
            v_scroll: 0,
            h_scroll: 0,
            right_view_lines:    Cell::new(0),
            right_content_lines: Cell::new(0),
            build_run_cmds: build_run_cmds(&info.targets),
            test_cmds:      test_cmds(),
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
            pkg_add_kind:    DepKind::Normal,
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
        // New output: return focus to the command list and follow the tail.
        self.focus    = Focus::Left;
        self.v_scroll = 0;
        self.h_scroll = 0;

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

        // In search mode: edit the query, run the search only on Enter.
        if self.pkg_search_mode {
            match key.code {
                KeyCode::Esc       => { self.pkg_search_mode = false; }
                KeyCode::Enter     => {
                    self.pkg_sel_search = 0;
                    self.trigger_search();
                }
                KeyCode::Backspace => { self.pkg_query.pop(); }
                KeyCode::Up        => self.move_search_sel(-1),
                KeyCode::Down      => self.move_search_sel(1),
                KeyCode::Char(c)   => { self.pkg_query.push(c); }
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
        if k.tab_2.matches(&key)    { self.switch_tab(Tab::Crate);  return; }
        if k.tab_3.matches(&key)    { self.switch_tab(Tab::Test);     return; }
        if k.tab_next.matches(&key) { self.next_tab(); return; }
        if k.tab_prev.matches(&key) { self.prev_tab(); return; }

        // Right-pane scroll mode: hjkl (and arrows) scroll the Output/Description.
        if self.focus == Focus::Right {
            if k.down.matches(&key) || key.code == KeyCode::Down  { self.scroll_v(1);  return; }
            if k.up.matches(&key)   || key.code == KeyCode::Up    { self.scroll_v(-1); return; }
            if k.focus_right.matches(&key) || key.code == KeyCode::Right {
                self.h_scroll = self.h_scroll.saturating_add(1);
                return;
            }
            if k.focus_left.matches(&key) || key.code == KeyCode::Left {
                // Scroll left; once at the left edge, hand focus back to the list.
                if self.h_scroll > 0 { self.h_scroll -= 1; } else { self.focus = Focus::Left; }
                return;
            }
            // Any other key falls through to the shared handlers below.
        } else if k.focus_right.matches(&key) || key.code == KeyCode::Right {
            self.focus_right();
            return;
        }

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

        // Crate tab actions
        if self.tab == Tab::Crate {
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
            // Cycle the target dependency section for adding crates.
            if k.pkg_profile.matches(&key) {
                self.pkg_add_kind = match self.pkg_add_kind {
                    DepKind::Normal => DepKind::Dev,
                    DepKind::Dev    => DepKind::Build,
                    DepKind::Build  => DepKind::Normal,
                };
                return;
            }
            if k.pkg_remove.matches(&key) && self.pkg_section == PkgSection::Installed {
                if let Some(dep) = self.pkg_deps.get(self.pkg_sel_inst).cloned() {
                    // Remove from the section the crate actually lives in.
                    let mut args = vec!["remove".to_string()];
                    if let Some(flag) = dep.kind.flag() {
                        args.push(flag.to_string());
                    }
                    args.push(dep.name);
                    self.run_cargo(args);
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
        self.focus    = Focus::Left;
        self.v_scroll = 0;
        self.h_scroll = 0;
    }

    /// Scroll the focused right pane vertically, clamped to its content.
    fn scroll_v(&mut self, delta: i32) {
        let max = self
            .right_content_lines
            .get()
            .saturating_sub(self.right_view_lines.get());
        let next = (self.v_scroll as i32 + delta).clamp(0, max as i32);
        self.v_scroll = next as u16;
    }

    /// Move focus into the right pane, starting at the current tail so the view
    /// does not jump when following live output.
    fn focus_right(&mut self) {
        self.focus = Focus::Right;
        let bottom = self
            .right_content_lines
            .get()
            .saturating_sub(self.right_view_lines.get());
        self.v_scroll = bottom;
        self.h_scroll = 0;
    }

    fn next_tab(&mut self) {
        self.switch_tab(match self.tab {
            Tab::BuildRun => Tab::Crate,
            Tab::Crate  => Tab::Test,
            Tab::Test     => Tab::BuildRun,
        });
    }

    fn prev_tab(&mut self) {
        self.switch_tab(match self.tab {
            Tab::BuildRun => Tab::Test,
            Tab::Crate  => Tab::BuildRun,
            Tab::Test     => Tab::Crate,
        });
    }

    fn move_sel(&mut self, delta: i32) {
        match self.tab {
            Tab::BuildRun => {
                self.br_sel = clamp_move(self.br_sel, delta, self.build_run_cmds.len());
            }
            Tab::Test => {
                self.test_sel = clamp_move(self.test_sel, delta, self.test_cmds.len());
            }
            Tab::Crate => match self.pkg_section {
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
                let cmd = self.build_run_cmds[self.br_sel].clone();
                // Library-only project: explain instead of running a doomed `cargo run`.
                if cmd.args.first().map(|s| s.as_str()) == Some(NO_BIN_SENTINEL) {
                    self.output = vec![
                        "  No binary target in this project.".to_string(),
                        "  This looks like a library crate — there is nothing to run.".to_string(),
                        "  Use build / check / test instead.".to_string(),
                    ];
                    return;
                }
                if cmd.prompt.is_none() {
                    self.output.clear();
                    self.run_cargo(cmd.args);
                }
            }
            Tab::Test => {
                let cmd = self.test_cmds[self.test_sel].clone();
                if cmd.prompt.is_none() {
                    self.output.clear();
                    self.run_cargo(cmd.args);
                }
            }
            Tab::Crate => {
                if self.pkg_section == PkgSection::Search {
                    if let Some(r) = self.pkg_results.get(self.pkg_sel_search).cloned() {
                        // Add to the currently selected dependency section.
                        let mut args = vec!["add".to_string()];
                        if let Some(flag) = self.pkg_add_kind.flag() {
                            args.push(flag.to_string());
                        }
                        args.push(r.name);
                        self.run_cargo(args);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(pkg: &str, name: &str) -> RunTarget {
        RunTarget { package: pkg.into(), name: name.into(), kind: RunKind::Bin }
    }
    fn example(pkg: &str, name: &str) -> RunTarget {
        RunTarget { package: pkg.into(), name: name.into(), kind: RunKind::Example }
    }

    fn rows(cmds: &[Cmd], section: &str) -> Vec<Vec<String>> {
        cmds.iter()
            .filter(|c| c.section == section)
            .map(|c| c.args.clone())
            .collect()
    }

    fn test_app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        let info = WorkspaceInfo {
            name: "t".into(),
            root: PathBuf::from("."),
            deps: vec![],
            targets: vec![],
        };
        App::new(info, crate::config::KeyConfig::default(), tx)
    }

    #[test]
    fn focus_right_starts_at_tail_and_scroll_clamps() {
        let mut app = test_app();
        app.right_content_lines.set(100);
        app.right_view_lines.set(10);

        app.focus_right();
        assert_eq!(app.focus, Focus::Right);
        assert_eq!(app.v_scroll, 90); // tail = content - view

        app.scroll_v(1);            // already at bottom -> clamped
        assert_eq!(app.v_scroll, 90);
        app.scroll_v(-5);
        assert_eq!(app.v_scroll, 85);
        app.scroll_v(-1000);        // clamped at top
        assert_eq!(app.v_scroll, 0);
    }

    #[test]
    fn switch_tab_resets_focus_and_scroll() {
        let mut app = test_app();
        app.focus = Focus::Right;
        app.v_scroll = 42;
        app.h_scroll = 7;
        app.switch_tab(Tab::Test);
        assert_eq!(app.focus, Focus::Left);
        assert_eq!(app.v_scroll, 0);
        assert_eq!(app.h_scroll, 0);
    }

    #[test]
    fn nothing_runnable_shows_placeholder() {
        let run = rows(&build_run_cmds(&[]), "RUN");
        assert_eq!(run.len(), 1);
        assert_eq!(run[0], vec![NO_BIN_SENTINEL.to_string()]);
        // The placeholder must never be a real `cargo run` invocation.
        assert!(!run[0].contains(&"run".to_string()));
    }

    #[test]
    fn single_bin_targets_it_explicitly() {
        let run = rows(&build_run_cmds(&[bin("app", "app")]), "RUN");
        assert_eq!(run, vec![
            vec!["run", "--bin", "app"],
            vec!["run", "--release", "--bin", "app"],
        ]);
    }

    #[test]
    fn multiple_bins_get_one_row_each() {
        let run = rows(&build_run_cmds(&[bin("w", "alpha"), bin("w", "beta")]), "RUN");
        assert_eq!(run, vec![
            vec!["run", "-p", "w", "--bin", "alpha"],
            vec!["run", "-p", "w", "--bin", "beta"],
            vec!["run", "--release", "-p", "w", "--bin", "alpha"],
            vec!["run", "--release", "-p", "w", "--bin", "beta"],
        ]);
    }

    #[test]
    fn examples_produce_run_example_rows() {
        let ex = rows(&build_run_cmds(&[example("demo", "main")]), "EXAMPLES");
        assert_eq!(ex, vec![
            vec!["run", "-p", "demo", "--example", "main"],
            vec!["run", "--release", "-p", "demo", "--example", "main"],
        ]);
    }

    #[test]
    fn examples_without_bins_do_not_show_placeholder() {
        let cmds = build_run_cmds(&[example("demo", "main")]);
        // No RUN placeholder when there is a runnable example.
        assert!(rows(&cmds, "RUN").is_empty());
        assert_eq!(rows(&cmds, "EXAMPLES").len(), 2);
    }

    #[test]
    fn bins_and_examples_coexist() {
        let cmds = build_run_cmds(&[bin("p", "app"), example("p", "main")]);
        assert_eq!(rows(&cmds, "RUN"), vec![
            vec!["run", "--bin", "app"],
            vec!["run", "--release", "--bin", "app"],
        ]);
        assert_eq!(rows(&cmds, "EXAMPLES"), vec![
            vec!["run", "-p", "p", "--example", "main"],
            vec!["run", "--release", "-p", "p", "--example", "main"],
        ]);
    }
}
