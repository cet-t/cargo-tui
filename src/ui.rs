use crate::{
    app::{App, PkgSection, Tab},
    crates_io::{CrateDetail, fmt_downloads},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

const SEL_STYLE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
const HEADER_STYLE: Style = Style::new()
    .fg(Color::Yellow)
    .add_modifier(Modifier::BOLD);
const MUTED_STYLE: Style  = Style::new().fg(Color::DarkGray);
const OK_STYLE: Style     = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
const ERR_STYLE: Style    = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
const WARN_STYLE: Style   = Style::new().fg(Color::Yellow);

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Layout: tabbar(3) / content / statusbar(1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_tabbar(frame, app, chunks[0]);

    match app.tab {
        Tab::BuildRun => render_build_run(frame, app, chunks[1]),
        Tab::Crate    => render_crate(frame, app, chunks[1]),
        Tab::Test     => render_test(frame, app, chunks[1]),
    }

    render_statusbar(frame, app, chunks[2]);
}

// ── Tab bar ───────────────────────────────────────────────────

fn render_tabbar(frame: &mut Frame, app: &App, area: Rect) {
    let titles = vec![
        Line::from(vec![
            Span::raw(" Build / Run "),
            Span::styled("[1]", MUTED_STYLE),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" Crate "),
            Span::styled("[2]", MUTED_STYLE),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" Test "),
            Span::styled("[3]", MUTED_STYLE),
            Span::raw(" "),
        ]),
    ];

    let tabs = Tabs::new(titles)
        .select(app.tab as usize)
        .block(Block::default().borders(Borders::ALL).title(format!(" cargo-tui — {} ", app.ws_name)))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

// ── Status bar ────────────────────────────────────────────────

fn render_statusbar(frame: &mut Frame, app: &App, area: Rect) {
    let text = if app.pkg_search_mode {
        " Searching: [Esc/Enter] done  [BS] delete".to_string()
    } else if app.running {
        " Running...  [K] kill".to_string()
    } else {
        match app.tab {
            Tab::Crate => format!(
                " [s] Search  [Enter] Add→{}  [p] Section  [d] Remove  [Tab] Switch  [q] Quit",
                app.pkg_add_kind.section(),
            ),
            _          => " [Enter] Run  [r] Re-run  [K] Kill  []/[] Tab  [q] Quit".to_string(),
        }
    };
    frame.render_widget(
        Paragraph::new(text).style(MUTED_STYLE),
        area,
    );
}

// ── Build/Run tab ─────────────────────────────────────────────

fn render_build_run(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] = split_lr(area, 40);

    // Left: command list with section headers
    let mut items: Vec<ListItem> = vec![];
    let mut last_section = "";
    for cmd in &app.build_run_cmds {
        if cmd.section.as_str() != last_section {
            if !last_section.is_empty() {
                items.push(ListItem::new(Line::from(Span::styled("  ─────────────────", MUTED_STYLE))));
            }
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {}", cmd.section),
                HEADER_STYLE,
            ))));
            last_section = cmd.section.as_str();
        }
        items.push(ListItem::new(format!("    {}", cmd.label)));
    }

    // Map command index to list item index (accounting for section headers)
    let list_idx = cmd_to_list_idx(app.br_sel, &app.build_run_cmds);
    let mut state = ListState::default().with_selected(Some(list_idx));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Commands "))
            .highlight_style(SEL_STYLE)
            .highlight_symbol("▶ "),
        left,
        &mut state,
    );

    // Right: output
    render_output(frame, app, right, " Output ");
}

// ── Test tab ──────────────────────────────────────────────────

fn render_test(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] = split_lr(area, 40);

    let mut items: Vec<ListItem> = vec![];
    items.push(ListItem::new(Line::from(Span::styled("  COMMANDS", HEADER_STYLE))));
    for cmd in &app.test_cmds {
        items.push(ListItem::new(format!("    {}", cmd.label)));
    }

    if !app.test_results.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled("  ─────────────────", MUTED_STYLE))));
        items.push(ListItem::new(Line::from(Span::styled("  RESULTS", HEADER_STYLE))));
        for r in &app.test_results {
            let (icon, style) = if r.ok { ("✓", OK_STYLE) } else { ("✗", ERR_STYLE) };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {} ", icon), style),
                Span::raw(&r.name),
            ])));
        }
    }

    let list_idx = app.test_sel + 1; // +1 for COMMANDS header
    let mut state = ListState::default().with_selected(Some(list_idx));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Test "))
            .highlight_style(SEL_STYLE)
            .highlight_symbol("▶ "),
        left,
        &mut state,
    );

    render_output(frame, app, right, " Output ");
}

// ── Crate tab ─────────────────────────────────────────────────

fn render_crate(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] = split_lr(area, 42);

    // Left column: Installed (top 40%) + Search (bottom 60%)
    let total_h = left.height;
    let inst_h  = (total_h as f32 * 0.40).floor() as u16;

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(inst_h), Constraint::Min(0)])
        .split(left);

    render_pkg_installed(frame, app, left_chunks[0]);
    render_pkg_search(frame, app, left_chunks[1]);

    // Right column: detail panel for the focused section
    let detail = match app.pkg_section {
        PkgSection::Installed => app.pkg_detail_inst.as_ref(),
        PkgSection::Search    => app.pkg_detail_srch.as_ref(),
    };
    render_pkg_detail(frame, detail, right);
}

fn render_pkg_installed(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.pkg_section == PkgSection::Installed;
    let border_style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    // Build the list grouped by dependency section, tracking where each
    // selectable crate row lands so the highlight maps to `pkg_sel_inst`.
    let mut items: Vec<ListItem> = vec![];
    let mut sel_list_idx = 0usize;
    let mut last_kind: Option<crate::workspace::DepKind> = None;
    for (i, d) in app.pkg_deps.iter().enumerate() {
        if last_kind != Some(d.kind) {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  [{}]", d.kind.section()),
                HEADER_STYLE,
            ))));
            last_kind = Some(d.kind);
        }
        if i == app.pkg_sel_inst {
            sel_list_idx = items.len();
        }
        items.push(ListItem::new(format!("    {:<26} {}", d.name, d.version)));
    }
    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  (no dependencies)",
            MUTED_STYLE,
        ))));
    }

    let mut state = ListState::default();
    if active && !app.pkg_deps.is_empty() {
        state.select(Some(sel_list_idx));
    }

    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(" Installed "),
            )
            .highlight_style(SEL_STYLE)
            .highlight_symbol("▶ "),
        area,
        &mut state,
    );
}

fn render_pkg_search(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.pkg_section == PkgSection::Search;
    let border_style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    // Inline search bar
    let search_bar = if app.pkg_search_mode {
        Line::from(vec![
            Span::raw("  Search: "),
            Span::styled(&app.pkg_query, Style::default().fg(Color::White)),
            Span::styled("▌", Style::default().fg(Color::Cyan)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  Search: ", MUTED_STYLE),
            Span::raw(&app.pkg_query),
        ])
    };

    // Result list
    let mut lines: Vec<Line> = vec![search_bar, Line::from("")];
    if app.pkg_loading {
        lines.push(Line::from(Span::styled("  Searching...", MUTED_STYLE)));
    } else if app.pkg_results.is_empty() {
        lines.push(Line::from(Span::styled("  (press s to search crates.io)", MUTED_STYLE)));
    } else {
        for (i, r) in app.pkg_results.iter().enumerate() {
            let dl = fmt_downloads(r.downloads);
            let is_sel = active && i == app.pkg_sel_search;
            let line = Line::from(vec![
                Span::raw(if is_sel { "▶ " } else { "  " }),
                Span::styled(
                    format!("{:<28} {:<14} ↓{}", r.name, r.version, dl),
                    if is_sel { SEL_STYLE } else { Style::default() },
                ),
            ]);
            lines.push(line);
        }
    }

    let inner_h = area.height.saturating_sub(2) as usize; // subtract borders
    let scroll  = if active && app.pkg_sel_search + 3 > inner_h {
        (app.pkg_sel_search + 3 - inner_h) as u16
    } else {
        0
    };

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(" Search "),
            )
            .scroll((scroll, 0)),
        area,
    );
}

fn render_pkg_detail(frame: &mut Frame, detail: Option<&CrateDetail>, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Detail ");

    let Some(d) = detail else {
        frame.render_widget(
            Paragraph::new(Span::styled("  (select an item)", MUTED_STYLE)).block(block),
            area,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![];

    lines.push(Line::from(Span::styled(
        format!("# {}", d.name),
        HEADER_STYLE,
    )));
    lines.push(Line::from(Span::styled(
        format!("  v{}", d.version),
        MUTED_STYLE,
    )));
    lines.push(Line::from(""));

    if !d.description.is_empty() {
        lines.push(Line::from(Span::styled("## description", MUTED_STYLE)));
        for word_line in wrap_text(&d.description, area.width.saturating_sub(4) as usize) {
            lines.push(Line::from(format!("  {}", word_line)));
        }
        lines.push(Line::from(""));
    }

    if !d.authors.is_empty() {
        lines.push(Line::from(Span::styled("### author(s)", MUTED_STYLE)));
        for a in &d.authors {
            lines.push(Line::from(format!("  {}", a)));
        }
        lines.push(Line::from(""));
    }

    if !d.deps.is_empty() {
        lines.push(Line::from(Span::styled("## dependencies", MUTED_STYLE)));
        for dep in &d.deps {
            lines.push(Line::from(format!("  {:<20} {}", dep.name, dep.req)));
        }
        lines.push(Line::from(""));
    }

    if !d.repository.is_empty() {
        lines.push(Line::from(Span::styled(&d.repository, MUTED_STYLE)));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ── Output panel ──────────────────────────────────────────────

fn render_output(frame: &mut Frame, app: &App, area: Rect, title: &str) {
    let lines: Vec<Line> = app
        .output
        .iter()
        .map(|l| {
            let style = if l.starts_with("error") {
                ERR_STYLE
            } else if l.starts_with("warning") {
                WARN_STYLE
            } else if l.contains("Finished") || l.starts_with("  ✓") {
                OK_STYLE
            } else {
                Style::default()
            };
            Line::from(Span::styled(l.as_str(), style))
        })
        .collect();

    let total = lines.len() as u16;
    let inner_h = area.height.saturating_sub(2);
    let scroll = total.saturating_sub(inner_h);

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::ALL).title(title))
            .scroll((scroll, 0)),
        area,
    );
}

// ── Utilities ─────────────────────────────────────────────────

/// Split area into left (left_pct%) and right columns.
fn split_lr(area: Rect, left_pct: u16) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(100 - left_pct),
        ])
        .split(area);
    [chunks[0], chunks[1]]
}

/// Convert a command index to the corresponding ListItem index (skipping section headers).
fn cmd_to_list_idx(cmd_idx: usize, cmds: &[crate::app::Cmd]) -> usize {
    let mut offset = 0usize;
    let mut last_section = "";
    for (i, cmd) in cmds.iter().enumerate() {
        if cmd.section.as_str() != last_section {
            if !last_section.is_empty() {
                offset += 1; // separator
            }
            offset += 1; // header
            last_section = cmd.section.as_str();
        }
        if i == cmd_idx {
            return offset;
        }
        offset += 1;
    }
    offset
}

/// Word-wrap `text` to lines of at most `width` characters.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 { return vec![text.to_string()]; }
    let mut lines = vec![];
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() > width {
            lines.push(cur.clone());
            cur = word.to_string();
        } else {
            cur.push(' ');
            cur.push_str(word);
        }
    }
    if !cur.is_empty() { lines.push(cur); }
    lines
}
