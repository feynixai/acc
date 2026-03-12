use crate::app::{App, InputMode, InputField, TreeEntry};
use crate::state::AgentStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

// ── Colors — refined dark theme ──

const BG: Color = Color::Rgb(22, 22, 26);
const BG_HEADER: Color = Color::Rgb(18, 18, 22);
const BG_HOVER: Color = Color::Rgb(38, 38, 46);
const BG_BAR: Color = Color::Rgb(28, 28, 34);
const FG: Color = Color::Rgb(190, 194, 204);
const FG_DIM: Color = Color::Rgb(92, 96, 108);
const FG_MUTED: Color = Color::Rgb(58, 62, 72);
const ACCENT: Color = Color::Rgb(100, 160, 255);
const GREEN: Color = Color::Rgb(72, 199, 142);
const YELLOW: Color = Color::Rgb(229, 192, 80);
const RED: Color = Color::Rgb(235, 87, 87);
const ORANGE: Color = Color::Rgb(230, 150, 60);

// ── Draw ──

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = area.width;

    // Responsive layout: skip elements that don't fit
    let show_buttons = area.height > 8;
    let show_bar = area.height > 6;

    let mut constraints = vec![
        Constraint::Length(2), // header
        Constraint::Min(2),   // tree
    ];
    if show_buttons { constraints.push(Constraint::Length(1)); } // buttons
    constraints.push(Constraint::Length(1)); // message/status bar

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    draw_header(f, app, chunks[idx], w); idx += 1;
    draw_tree(f, app, chunks[idx], w);   idx += 1;
    if show_buttons { draw_buttons(f, chunks[idx], w); idx += 1; }
    if show_bar { draw_status_bar(f, app, chunks[idx], w); }

    if matches!(app.input_mode, InputMode::AddFolder | InputMode::AddAgent) {
        draw_input(f, app);
    }
    if app.confirm_delete.is_some() {
        draw_confirm(f, app);
    }
}

// ── Header: workspace name + counts on one line ──

fn draw_header(f: &mut Frame, app: &App, area: Rect, w: u16) {
    let agents = app.state.all_agents();
    let active = agents.iter().filter(|a| a.status == AgentStatus::Active).count();
    let idle = agents.iter().filter(|a| a.status == AgentStatus::Idle).count();
    let stopped = agents.iter().filter(|a| a.status == AgentStatus::Stopped).count();

    let ws_name = std::path::Path::new(&app.workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.workspace.clone());

    // Truncate workspace name if needed
    let max_name = (w as usize).saturating_sub(2);
    let display_name = if ws_name.len() > max_name {
        format!("{}...", &ws_name[..max_name.saturating_sub(3)])
    } else {
        ws_name
    };

    let mut line2 = vec![
        Span::styled(format!(" {active}"), Style::default().fg(GREEN)),
    ];
    if w > 12 {
        line2.push(Span::styled(format!(" {idle}"), Style::default().fg(YELLOW)));
    }
    if w > 18 {
        line2.push(Span::styled(format!(" {stopped}"), Style::default().fg(FG_DIM)));
    }

    let p = Paragraph::new(vec![
        Line::from(Span::styled(
            format!(" {display_name}"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(line2),
    ]).style(Style::default().bg(BG_HEADER));
    f.render_widget(p, area);
}

// ── Tree ──

fn draw_tree(f: &mut Frame, app: &App, area: Rect, w: u16) {
    let max_name_len = (w as usize).saturating_sub(10); // room for icon + indent + count

    let items: Vec<ListItem> = app.tree_items().iter().map(|entry| match entry {
        TreeEntry::Folder(fi) => {
            let folder = &app.state.folders[*fi];
            let total = folder.agents.len();
            let is_active = app.active_folder_idx == Some(*fi);
            let fc = if is_active { ACCENT } else { FG };

            let arrow = if is_active { "▾" } else { "▸" };
            let name = truncate(&folder.name, max_name_len);

            let mut spans = vec![
                Span::styled(format!(" {arrow} "), Style::default().fg(FG_MUTED)),
                Span::styled(name, Style::default().fg(fc).add_modifier(Modifier::BOLD)),
            ];
            if w > 14 {
                spans.push(Span::styled(
                    format!(" {total}"),
                    Style::default().fg(FG_MUTED),
                ));
            }

            ListItem::new(Line::from(spans))
        }
        TreeEntry::Agent(fi, ai) => {
            let agent = &app.state.folders[*fi].agents[*ai];
            let (icon, ic) = match agent.status {
                AgentStatus::Active => ("●", GREEN),
                AgentStatus::Idle => ("●", YELLOW),
                AgentStatus::Stopped => ("○", FG_MUTED),
            };
            let key = format!("{}/{}", agent.folder, agent.name);
            let is_focused = app.focused_agent_key.as_deref() == Some(&key);
            let nc = if is_focused { ACCENT } else { FG };
            let nm = if is_focused { Modifier::BOLD } else { Modifier::empty() };

            let agent_max = max_name_len.saturating_sub(4);
            let name = truncate(&agent.name, agent_max);

            let mut spans = vec![
                Span::styled("   ", Style::default()),
                Span::styled(icon, Style::default().fg(ic)),
                Span::raw(" "),
                Span::styled(name, Style::default().fg(nc).add_modifier(nm)),
            ];

            // Git branch only if enough room
            if !agent.git_branch.is_empty() && w > 25 {
                let branch_max = (w as usize).saturating_sub(agent.name.len() + 12);
                let branch = truncate(&agent.git_branch, branch_max);
                spans.push(Span::styled(
                    format!(" {branch}"),
                    Style::default().fg(FG_MUTED),
                ));
            }

            ListItem::new(Line::from(spans))
        }
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE).style(Style::default().bg(BG)))
        .highlight_style(Style::default().bg(BG_HOVER))
        .highlight_symbol(">");

    f.render_stateful_widget(list, area, &mut app.list_state.clone());
}

// ── Buttons row — compact icons ──

fn draw_buttons(f: &mut Frame, area: Rect, w: u16) {
    let mut spans = vec![
        Span::styled(" +f", Style::default().fg(ACCENT)),
        Span::styled(" +a", Style::default().fg(GREEN)),
    ];
    if w > 14 {
        spans.push(Span::styled(" -x", Style::default().fg(RED)));
    }
    let p = Paragraph::new(Line::from(spans)).style(Style::default().bg(BG_HEADER));
    f.render_widget(p, area);
}

// ── Status bar: message OR mini help — responsive ──

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, w: u16) {
    let text = match app.input_mode {
        InputMode::Command => {
            Line::from(vec![
                Span::styled(":", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(&app.cmd_buf, Style::default().fg(FG)),
                Span::styled("_", Style::default().fg(ACCENT)),
            ])
        }
        InputMode::Search => {
            Line::from(vec![
                Span::styled("/", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(&app.search_buf, Style::default().fg(FG)),
                Span::styled("_", Style::default().fg(ACCENT)),
            ])
        }
        InputMode::SendPrompt => {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(&app.cmd_buf, Style::default().fg(FG)),
                Span::styled("_", Style::default().fg(ACCENT)),
            ])
        }
        _ => {
            if let Some(ref msg) = app.message {
                Line::from(Span::styled(format!(" {msg}"), Style::default().fg(ORANGE)))
            } else {
                // Mini help — adaptive to width
                build_help_line(w)
            }
        }
    };
    let p = Paragraph::new(text).style(Style::default().bg(BG_BAR));
    f.render_widget(p, area);
}

/// Build a help line that fits the available width.
fn build_help_line(w: u16) -> Line<'static> {
    let w = w as usize;

    // Priority tiers of hints — show as many as fit
    let hints: &[(&str, &str)] = &[
        ("Enter", "go"),
        ("[/]", "win"),
        ("n/p", "pane"),
        ("f", "fold"),
        ("a", "add"),
        ("x", "del"),
        ("u", "undo"),
        ("r", "rst"),
        (":", "cmd"),
        ("/", "find"),
        ("q", "quit"),
    ];

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 1; // leading space

    for (key, label) in hints {
        let needed = key.len() + label.len() + 2; // " key label"
        if used + needed > w { break; }
        spans.push(Span::styled(
            format!(" {key}"),
            Style::default().fg(FG_DIM),
        ));
        spans.push(Span::styled(
            format!("{label}"),
            Style::default().fg(FG_MUTED),
        ));
        used += needed;
    }

    Line::from(spans)
}

fn draw_input(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = (area.width.saturating_sub(4)).min(44);
    let h = 7;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let (title, hint) = match (&app.input_mode, &app.input_field) {
        (InputMode::AddFolder, InputField::Name) => ("Folder Name", "project name"),
        (InputMode::AddFolder, InputField::Path) => ("Directory", "Tab to complete"),
        (InputMode::AddAgent, InputField::Name) => ("Agent Name", "e.g. fix-auth"),
        (InputMode::AddAgent, InputField::Command) => ("Command", "default: claude"),
        (InputMode::AddAgent, InputField::Path) => ("Directory", "Tab to complete"),
        _ => ("Input", ""),
    };

    let step = match (&app.input_mode, app.input_stage) {
        (InputMode::AddFolder, 0) => "1/2",
        (InputMode::AddFolder, _) => "2/2",
        (InputMode::AddAgent, 0) => "1/3",
        (InputMode::AddAgent, 1) => "2/3",
        (InputMode::AddAgent, _) => "3/3",
        _ => "",
    };

    let text = vec![
        Line::from(vec![
            Span::styled(format!(" {title}"), Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {step}"), Style::default().fg(FG_MUTED)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(ACCENT)),
            Span::styled(&app.input_buf, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(ACCENT)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("  {hint}"), Style::default().fg(FG_MUTED)),
            Span::styled("  Esc cancel", Style::default().fg(FG_MUTED)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(44, 44, 54)))
        .style(Style::default().bg(Color::Rgb(30, 30, 38)));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

fn draw_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = (area.width.saturating_sub(4)).min(38);
    let h = 5;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let name = match &app.confirm_delete {
        Some(TreeEntry::Folder(fi)) => {
            if *fi < app.state.folders.len() {
                app.state.folders[*fi].name.clone()
            } else { "?".into() }
        }
        Some(TreeEntry::Agent(fi, ai)) => {
            if *fi < app.state.folders.len() && *ai < app.state.folders[*fi].agents.len() {
                app.state.folders[*fi].agents[*ai].name.clone()
            } else { "?".into() }
        }
        None => return,
    };

    let text = vec![
        Line::from(Span::styled(
            format!(" Delete \"{name}\"?"),
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(" yes  ", Style::default().fg(FG_DIM)),
            Span::styled("Esc", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(" no", Style::default().fg(FG_DIM)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(80, 36, 36)))
        .style(Style::default().bg(Color::Rgb(36, 26, 28)));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

// ── Helpers ──

fn truncate(s: &str, max: usize) -> String {
    if max < 4 {
        return s.chars().take(max).collect();
    }
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}..", &s[..max.saturating_sub(2)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{make_app, make_agent, make_folder};
    use crate::state::AgentStatus;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut output = String::new();
        for y in 0..height {
            for x in 0..width {
                output.push(buf.cell((x, y)).unwrap().symbol().chars().next().unwrap_or(' '));
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn test_draw_empty_app() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 30, 10);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_draw_with_folders() {
        let app = make_app(vec![
            make_folder("BB", vec![
                make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
            ]),
            make_folder("Arun", vec![]),
        ]);
        let output = render_to_string(&app, 30, 12);
        assert!(output.contains("BB"));
    }

    #[test]
    fn test_draw_with_agents() {
        let app = make_app(vec![
            make_folder("Project", vec![
                make_agent("fix-auth", "Project", AgentStatus::Active, Some("%1")),
                make_agent("idle-one", "Project", AgentStatus::Idle, Some("%2")),
                make_agent("stopped", "Project", AgentStatus::Stopped, None),
            ]),
        ]);
        let output = render_to_string(&app, 30, 15);
        assert!(output.contains("fix-auth"));
        assert!(output.contains("idle-one"));
        assert!(output.contains("stopped"));
    }

    #[test]
    fn test_draw_narrow_sidebar() {
        let app = make_app(vec![
            make_folder("VeryLongFolderNameThatOverflows", vec![
                make_agent("agent-with-long-name-too", "VeryLongFolderNameThatOverflows", AgentStatus::Active, Some("%1")),
            ]),
        ]);
        // 15 chars wide — should truncate, not panic
        let output = render_to_string(&app, 15, 10);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_draw_tiny_terminal() {
        let app = make_app(vec![make_folder("BB", vec![])]);
        let _output = render_to_string(&app, 8, 5);
    }

    #[test]
    fn test_draw_command_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "quit".into();
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("quit"));
    }

    #[test]
    fn test_draw_search_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Search;
        app.search_buf = "fix".into();
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("fix"));
    }

    #[test]
    fn test_draw_send_prompt_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::SendPrompt;
        app.cmd_buf = "hello".into();
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_draw_with_message() {
        let mut app = make_app(vec![]);
        app.set_message("Test message");
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("Test message"));
    }

    #[test]
    fn test_draw_confirm_delete_folder() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.confirm_delete = Some(TreeEntry::Folder(0));
        let output = render_to_string(&app, 50, 15);
        assert!(output.contains("Delete"));
        assert!(output.contains("BB"));
    }

    #[test]
    fn test_draw_confirm_delete_agent() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("fix-auth", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.confirm_delete = Some(TreeEntry::Agent(0, 0));
        let output = render_to_string(&app, 50, 15);
        assert!(output.contains("fix-auth"));
    }

    #[test]
    fn test_draw_input_popup() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::AddFolder;
        app.input_field = InputField::Name;
        app.input_buf = "proj".into();
        let output = render_to_string(&app, 50, 15);
        assert!(output.contains("proj"));
    }

    #[test]
    fn test_help_line_adapts_to_width() {
        let wide = build_help_line(80);
        let narrow = build_help_line(20);
        // Wide should have more spans than narrow
        assert!(wide.spans.len() > narrow.spans.len());
    }

    #[test]
    fn test_help_line_empty_at_zero() {
        let line = build_help_line(0);
        assert!(line.spans.is_empty());
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 8), "hello ..");
    }

    #[test]
    fn test_truncate_tiny() {
        assert_eq!(truncate("abcdef", 3), "abc");
    }

    #[test]
    fn test_active_folder_arrow() {
        let mut app = make_app(vec![
            make_folder("BB", vec![]),
            make_folder("Arun", vec![]),
        ]);
        app.active_folder_idx = Some(0);
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains('▾'));
        assert!(output.contains('▸'));
    }

    #[test]
    fn test_focused_agent_shown() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("focus-me", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.focused_agent_key = Some("BB/focus-me".into());
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("focus-me"));
    }

    #[test]
    fn test_git_branch_shown_wide() {
        let mut agent = make_agent("a1", "BB", AgentStatus::Active, Some("%1"));
        agent.git_branch = "main".into();
        let app = make_app(vec![make_folder("BB", vec![agent])]);
        let output = render_to_string(&app, 40, 10);
        assert!(output.contains("main"));
    }

    #[test]
    fn test_git_branch_hidden_narrow() {
        let mut agent = make_agent("a1", "BB", AgentStatus::Active, Some("%1"));
        agent.git_branch = "main".into();
        let app = make_app(vec![make_folder("BB", vec![agent])]);
        let output = render_to_string(&app, 15, 10);
        // Branch should be hidden on narrow width
        assert!(!output.contains("main"));
    }

    #[test]
    fn test_many_folders() {
        let folders: Vec<_> = (0..50)
            .map(|i| make_folder(&format!("F-{i}"), vec![
                make_agent(&format!("a-{i}"), &format!("F-{i}"), AgentStatus::Active, Some(&format!("%{i}"))),
            ]))
            .collect();
        let app = make_app(folders);
        let output = render_to_string(&app, 30, 15);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_header_workspace_name() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("workspace"));
    }

    #[test]
    fn test_buttons_shown_tall() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 30, 12);
        assert!(output.contains("+f"));
    }

    #[test]
    fn test_buttons_hidden_short() {
        let app = make_app(vec![]);
        // Very short terminal — buttons should be hidden
        let output = render_to_string(&app, 30, 6);
        assert!(!output.contains("+f"));
    }
}
