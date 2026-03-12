use crate::app::{App, InputMode, InputField, TreeEntry};
use crate::state::AgentStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

// ── Colors — monochrome minimal with grey hierarchy ──

const FG_DIM: Color = Color::Indexed(242);     // dark grey  — metadata, hints
const FG_MID: Color = Color::Indexed(249);     // light grey — agent names
const FG_BRIGHT: Color = Color::White;         // bright     — folders, selected
const GREEN: Color = Color::Indexed(71);       // muted green — active dot
const YELLOW: Color = Color::Indexed(179);     // muted amber — idle dot
const RED: Color = Color::Indexed(131);        // muted red   — delete confirm

// ── Draw ──

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = area.width;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(2),   // tree
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_header(f, app, chunks[0], w);
    draw_tree(f, app, chunks[1], w);
    draw_status_bar(f, app, chunks[2], w);

    if matches!(app.input_mode, InputMode::AddFolder | InputMode::AddAgent) {
        draw_input(f, app);
    }
    if app.input_mode == InputMode::Rename {
        draw_rename(f, app);
    }
    if app.input_mode == InputMode::SendPrompt {
        draw_send_prompt(f, app);
    }
    if app.confirm_delete.is_some() {
        draw_confirm(f, app);
    }
    if app.show_help {
        draw_help_overlay(f);
    }
}

// ── Header ──

fn draw_header(f: &mut Frame, app: &App, area: Rect, w: u16) {
    let agents = app.state.all_agents();
    let active = agents.iter().filter(|a| a.status == AgentStatus::Active).count();
    let idle = agents.iter().filter(|a| a.status == AgentStatus::Idle).count();

    let ws_name = std::path::Path::new(&app.workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.workspace.clone());

    // Reserve space for counts: " BB  2● 1○"
    let counts_len = format!("  {active}● {idle}○").len() + 2;
    let max_name = (w as usize).saturating_sub(counts_len + 2);
    let display_name = truncate(&ws_name, max_name);

    let spans = vec![
        Span::styled(
            format!("  {display_name}"),
            Style::default().fg(FG_BRIGHT),
        ),
    ];

    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

// ── Tree ──

fn draw_tree(f: &mut Frame, app: &App, area: Rect, w: u16) {
    let max_name_len = (w as usize).saturating_sub(10);

    let items: Vec<ListItem> = app.tree_items().iter().map(|entry| match entry {
        TreeEntry::Folder(fi) => {
            let folder = &app.state.folders[*fi];
            let total = folder.agents.len();
            let is_active = app.active_folder_idx == Some(*fi);

            let arrow = if is_active { "▾" } else { "▸" };
            let name = truncate(&folder.name, max_name_len);

            let mut spans = vec![
                Span::styled(format!(" {arrow} "), Style::default().fg(FG_DIM)),
                Span::styled(name, Style::default().fg(FG_BRIGHT)),
            ];
            if w > 14 {
                spans.push(Span::styled(
                    format!(" {total}"),
                    Style::default().fg(FG_DIM),
                ));
            }

            ListItem::new(Line::from(spans))
        }
        TreeEntry::Agent(fi, ai) => {
            let agent = &app.state.folders[*fi].agents[*ai];
            let (icon, ic) = if agent.hidden {
                ("◌", FG_DIM)
            } else {
                match agent.status {
                    AgentStatus::Active => ("●", GREEN),
                    AgentStatus::Idle => ("●", YELLOW),
                    AgentStatus::Stopped => ("○", FG_DIM),
                }
            };
            let key = format!("{}/{}", agent.folder, agent.name);
            let is_focused = app.focused_agent_key.as_deref() == Some(&key);
            let nc = if agent.hidden { FG_DIM } else if is_focused { FG_BRIGHT } else { FG_MID };
            let nm = if is_focused && !agent.hidden { Modifier::BOLD } else { Modifier::empty() };

            let agent_max = max_name_len.saturating_sub(8);
            let name = truncate(&agent.name, agent_max);

            // 1-based pane number for quick jumping
            let pane_num = ai + 1;

            let mut spans = vec![
                Span::styled(
                    format!("    {pane_num} "),
                    Style::default().fg(FG_DIM),
                ),
                Span::styled(name, Style::default().fg(nc).add_modifier(nm)),
            ];

            if !agent.git_branch.is_empty() && w > 25 {
                let branch_max = (w as usize).saturating_sub(agent.name.len() + 14);
                let branch = truncate(&agent.git_branch, branch_max);
                spans.push(Span::styled(
                    format!(" {branch}"),
                    Style::default().fg(FG_DIM),
                ));
            }

            // Right-align status dot with margin from edge
            let current_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let pad = (w as usize).saturating_sub(current_len + 3);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.push(Span::styled(icon, Style::default().fg(ic)));
            spans.push(Span::raw(" "));

            ListItem::new(Line::from(spans))
        }
    }).collect();

    // Focus-aware highlight
    let hl_bg = if app.sidebar_focused {
        Color::Indexed(237) // visible highlight
    } else {
        Color::Indexed(235) // subtle highlight
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(Style::default().bg(hl_bg))
        .highlight_symbol(">");

    f.render_stateful_widget(list, area, &mut app.list_state.clone());
}

// ── Status bar ──

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, _w: u16) {
    let text = match app.input_mode {
        InputMode::Command => {
            Line::from(vec![
                Span::styled(":", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
                Span::raw(&app.cmd_buf),
                Span::styled("_", Style::default().fg(FG_BRIGHT)),
            ])
        }
        InputMode::Search => {
            Line::from(vec![
                Span::styled("/", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
                Span::raw(&app.search_buf),
                Span::styled("_", Style::default().fg(FG_BRIGHT)),
            ])
        }
        InputMode::SendPrompt => {
            // SendPrompt now uses a popup; status bar just shows hint
            Line::from(Span::styled(" > sending...", Style::default().fg(FG_DIM)))
        }
        _ => {
            if let Some(ref msg) = app.message {
                Line::from(Span::styled(format!(" {msg}"), Style::default().fg(YELLOW)))
            } else {
                Line::from(Span::styled(" ? help", Style::default().fg(FG_DIM)))
            }
        }
    };
    let p = Paragraph::new(text);
    f.render_widget(p, area);
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
            Span::styled(format!(" {title}"), Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {step}"), Style::default().fg(FG_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(FG_BRIGHT)),
            Span::raw(&app.input_buf),
            Span::styled("_", Style::default().fg(FG_BRIGHT)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("  {hint}"), Style::default().fg(FG_DIM)),
            Span::styled("  Esc cancel", Style::default().fg(FG_DIM)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FG_DIM));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

fn draw_rename(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = (area.width.saturating_sub(4)).min(44);
    let h = 5;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(Span::styled(
            " Rename",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(FG_BRIGHT)),
            Span::raw(&app.input_buf),
            Span::styled("_", Style::default().fg(FG_BRIGHT)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FG_DIM));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

fn draw_send_prompt(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = (area.width.saturating_sub(4)).min(44);
    let h = 7;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(Span::styled(
            " Send to agent",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(FG_BRIGHT)),
            Span::raw(&app.cmd_buf),
            Span::styled("_", Style::default().fg(FG_BRIGHT)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled(" send  ", Style::default().fg(FG_DIM)),
            Span::styled("Esc", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(FG_DIM)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FG_DIM));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

fn draw_help_overlay(f: &mut Frame) {
    let area = f.area();
    let w = (area.width.saturating_sub(2)).min(36);
    let h = 33u16.min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let dim = Style::default().fg(FG_DIM);
    let key_style = Style::default().fg(FG_MID);
    let section = Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD);

    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled("  Navigation", section)),
        Line::from(vec![Span::styled("  \u{2191}/\u{2193}       ", key_style), Span::styled("Select", dim)]),
        Line::from(vec![Span::styled("  Enter     ", key_style), Span::styled("Focus pane", dim)]),
        Line::from(vec![Span::styled("  1-9       ", key_style), Span::styled("Jump to pane #", dim)]),
        Line::from(vec![Span::styled("  [/]       ", key_style), Span::styled("Prev/next window", dim)]),
        Line::from(vec![Span::styled("  n/p       ", key_style), Span::styled("Prev/next pane", dim)]),
        Line::raw(""),
        Line::from(Span::styled("  Panes", section)),
        Line::from(vec![Span::styled("  a         ", key_style), Span::styled("New agent", dim)]),
        Line::from(vec![Span::styled("  c         ", key_style), Span::styled("Hide pane (bg)", dim)]),
        Line::from(vec![Span::styled("  d         ", key_style), Span::styled("Close pane (kill)", dim)]),
        Line::from(vec![Span::styled("  h/v       ", key_style), Span::styled("Split horiz/vert", dim)]),
        Line::from(vec![Span::styled("  s         ", key_style), Span::styled("Stop agent", dim)]),
        Line::from(vec![Span::styled("  r         ", key_style), Span::styled("Restart agent", dim)]),
        Line::raw(""),
        Line::from(Span::styled("  Workspace", section)),
        Line::from(vec![Span::styled("  f         ", key_style), Span::styled("Add folder", dim)]),
        Line::from(vec![Span::styled("  x         ", key_style), Span::styled("Delete", dim)]),
        Line::from(vec![Span::styled("  R         ", key_style), Span::styled("Rename", dim)]),
        Line::from(vec![Span::styled("  u         ", key_style), Span::styled("Undo delete", dim)]),
        Line::raw(""),
        Line::from(Span::styled("  Other", section)),
        Line::from(vec![Span::styled("  :         ", key_style), Span::styled("Command mode", dim)]),
        Line::from(vec![Span::styled("  /         ", key_style), Span::styled("Search", dim)]),
        Line::from(vec![Span::styled("  >         ", key_style), Span::styled("Send prompt", dim)]),
        Line::from(vec![Span::styled("  q         ", key_style), Span::styled("Quit sidebar", dim)]),
        Line::raw(""),
        Line::from(Span::styled("  Default agent cmd: \"claude\"", dim)),
        Line::raw(""),
        Line::from(Span::styled("  Press ? or Esc to close", dim)),
    ];

    // Trim lines to fit available height (account for border)
    let max_lines = (h as usize).saturating_sub(2);
    lines.truncate(max_lines);

    let block = Block::default()
        .title(" Keybindings ")
        .title_style(Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FG_DIM));

    let p = Paragraph::new(lines).block(block);
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
            Span::styled("  Enter", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled(" yes  ", Style::default().fg(FG_DIM)),
            Span::styled("Esc", Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled(" no", Style::default().fg(FG_DIM)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RED));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

// ── Helpers ──

fn truncate(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        return s.to_string();
    }
    if max < 4 {
        return s.chars().take(max).collect();
    }
    let truncated: String = s.chars().take(max.saturating_sub(2)).collect();
    format!("{truncated}..")
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
        let output = render_to_string(&app, 50, 15);
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
    fn test_draw_rename_popup() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.input_mode = InputMode::Rename;
        app.input_buf = "NewName".into();
        let output = render_to_string(&app, 50, 15);
        assert!(output.contains("Rename"));
        assert!(output.contains("NewName"));
    }

    #[test]
    fn test_status_bar_shows_help_hint() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 30, 10);
        assert!(output.contains("? help"));
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
    fn test_help_overlay_renders() {
        let mut app = make_app(vec![]);
        app.show_help = true;
        let output = render_to_string(&app, 50, 40);
        assert!(output.contains("Keybindings"));
    }

    #[test]
    fn test_send_prompt_popup_renders() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::SendPrompt;
        app.cmd_buf = "hello".into();
        let output = render_to_string(&app, 50, 15);
        assert!(output.contains("Send to agent"));
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_focus_aware_highlight() {
        // Just verify both focus states render without panic
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.sidebar_focused = true;
        let _ = render_to_string(&app, 30, 10);
        app.sidebar_focused = false;
        let _ = render_to_string(&app, 30, 10);
    }
}
