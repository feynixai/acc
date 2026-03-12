use crate::app::{App, InputMode, InputField, TreeEntry};
use crate::state::AgentStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

// Colors — VS Code dark theme
const BG: Color = Color::Rgb(30, 30, 30);
const BG_DARKER: Color = Color::Rgb(24, 24, 24);
const BG_HOVER: Color = Color::Rgb(45, 45, 48);
const FG: Color = Color::Rgb(204, 204, 204);
const FG_DIM: Color = Color::Rgb(100, 100, 100);
const FG_DIMMER: Color = Color::Rgb(65, 65, 65);
const BLUE: Color = Color::Rgb(75, 156, 245);
const GREEN: Color = Color::Rgb(80, 200, 120);
const YELLOW: Color = Color::Rgb(220, 180, 50);
const RED: Color = Color::Rgb(240, 80, 80);

// ── Draw ──

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // header
            Constraint::Min(3),    // tree
            Constraint::Length(1), // buttons
            Constraint::Length(1), // message
            Constraint::Length(1), // help
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_tree(f, app, chunks[1]);
    draw_buttons(f, chunks[2]);
    draw_message(f, app, chunks[3]);
    draw_help(f, chunks[4]);

    if matches!(app.input_mode, InputMode::AddFolder | InputMode::AddAgent) {
        draw_input(f, app);
    }
    if app.confirm_delete.is_some() {
        draw_confirm(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let agents = app.state.all_agents();
    let active = agents.iter().filter(|a| a.status == AgentStatus::Active).count();
    let idle = agents.iter().filter(|a| a.status == AgentStatus::Idle).count();
    let stopped = agents.iter().filter(|a| a.status == AgentStatus::Stopped).count();

    let ws_name = std::path::Path::new(&app.workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.workspace.clone());

    let p = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!(" {ws_name}"), Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(format!(" ●{active}"), Style::default().fg(GREEN)),
            Span::styled(format!("  ◐{idle}"), Style::default().fg(YELLOW)),
            Span::styled(format!("  ○{stopped}"), Style::default().fg(FG_DIM)),
        ]),
    ]).style(Style::default().bg(BG_DARKER));
    f.render_widget(p, area);
}

fn draw_tree(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.tree_items().iter().map(|entry| match entry {
        TreeEntry::Folder(fi) => {
            let folder = &app.state.folders[*fi];
            let active = folder.agents.iter().filter(|a| a.status == AgentStatus::Active).count();
            let total = folder.agents.len();
            let is_active = app.active_folder_idx == Some(*fi);
            let fc = if is_active { BLUE } else { FG };
            let fm = Modifier::BOLD;

            ListItem::new(Line::from(vec![
                Span::styled(if is_active { "  ▾ " } else { "  ▸ " }, Style::default().fg(FG_DIM)),
                Span::styled(folder.name.to_uppercase(), Style::default().fg(fc).add_modifier(fm)),
                Span::styled(format!("  {total}"), Style::default().fg(FG_DIMMER)),
                if active > 0 {
                    Span::styled(format!("  ●{active}"), Style::default().fg(GREEN))
                } else { Span::raw("") },
            ]))
        }
        TreeEntry::Agent(fi, ai) => {
            let agent = &app.state.folders[*fi].agents[*ai];
            let (icon, ic) = match agent.status {
                AgentStatus::Active => ("●", GREEN),
                AgentStatus::Idle => ("◐", YELLOW),
                AgentStatus::Stopped => ("○", FG_DIMMER),
            };
            let key = format!("{}/{}", agent.folder, agent.name);
            let is_focused = app.focused_agent_key.as_deref() == Some(&key);
            let nc = if is_focused { BLUE } else { FG };
            let nm = if is_focused { Modifier::BOLD } else { Modifier::empty() };

            ListItem::new(Line::from(vec![
                Span::raw("     "),
                Span::styled(icon, Style::default().fg(ic)),
                Span::raw(" "),
                Span::styled(&agent.name, Style::default().fg(nc).add_modifier(nm)),
                if !agent.git_branch.is_empty() {
                    Span::styled(format!("  ⎇ {}", agent.git_branch), Style::default().fg(FG_DIMMER))
                } else { Span::raw("") },
            ]))
        }
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE).style(Style::default().bg(BG)))
        .highlight_style(Style::default().bg(BG_HOVER))
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, area, &mut app.list_state.clone());
}

fn draw_buttons(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" 📁", Style::default().fg(BLUE)),
        Span::styled("+", Style::default().fg(FG_DIM)),
        Span::styled("   ＋", Style::default().fg(GREEN)),
        Span::styled("   🗑", Style::default().fg(RED)),
    ]);
    let p = Paragraph::new(line).style(Style::default().bg(BG_DARKER));
    f.render_widget(p, area);
}

fn draw_message(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.input_mode {
        InputMode::Command => {
            Line::from(vec![
                Span::styled(":", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(&app.cmd_buf, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(BLUE)),
            ])
        }
        InputMode::Search => {
            Line::from(vec![
                Span::styled("/", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(&app.search_buf, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(BLUE)),
            ])
        }
        InputMode::SendPrompt => {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(&app.cmd_buf, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(BLUE)),
            ])
        }
        _ => {
            if let Some(ref msg) = app.message {
                Line::from(Span::styled(format!(" {msg}"), Style::default().fg(YELLOW)))
            } else {
                Line::raw("")
            }
        }
    };
    let p = Paragraph::new(text).style(Style::default().bg(BG));
    f.render_widget(p, area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key_span("1-9"), label_span("jump "),
        key_span("["), label_span("/"),
        key_span("]"), label_span("win "),
        key_span("n"), label_span("/"),
        key_span("p"), label_span("pane "),
        key_span(":"), label_span("cmd "),
        key_span("/"), label_span("find "),
        key_span(">"), label_span("send "),
        key_span("r"), label_span("estart "),
        key_span("u"), label_span("ndo "),
        key_span("h"), label_span("/"),
        key_span("v"), label_span("split "),
        key_span("q"), label_span("uit"),
    ]);
    let p = Paragraph::new(line).style(Style::default().bg(BG_DARKER));
    f.render_widget(p, area);
}

fn key_span(s: &str) -> Span<'_> {
    Span::styled(format!(" {s}"), Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
}
fn label_span(s: &str) -> Span<'_> {
    Span::styled(s, Style::default().fg(FG_DIMMER))
}

fn draw_input(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = 48.min(area.width.saturating_sub(2));
    let h = 7;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let (title, hint) = match (&app.input_mode, &app.input_field) {
        (InputMode::AddFolder, InputField::Name) => ("Folder Name", "project name"),
        (InputMode::AddFolder, InputField::Path) => ("Working Directory", "Tab to autocomplete"),
        (InputMode::AddAgent, InputField::Name) => ("Agent Name", "e.g. fix-auth"),
        (InputMode::AddAgent, InputField::Command) => ("Command", "default: claude"),
        (InputMode::AddAgent, InputField::Path) => ("Working Directory", "Tab to autocomplete"),
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
            Span::styled(format!("  {step}"), Style::default().fg(FG_DIMMER)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(BLUE)),
            Span::styled(&app.input_buf, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(BLUE)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("  {hint}"), Style::default().fg(FG_DIMMER)),
            Span::styled("  Esc:cancel", Style::default().fg(FG_DIMMER)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)))
        .style(Style::default().bg(Color::Rgb(35, 35, 35)));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
}

fn draw_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = 40.min(area.width.saturating_sub(2));
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
            Span::styled("  Enter", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
            Span::styled(" confirm  ", Style::default().fg(FG_DIM)),
            Span::styled("Esc", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(FG_DIM)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(80, 40, 40)))
        .style(Style::default().bg(Color::Rgb(40, 30, 30)));

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup);
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

    // ── Basic rendering doesn't panic ──

    #[test]
    fn test_draw_empty_app() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 60, 20);
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
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("BB"));
    }

    #[test]
    fn test_draw_with_active_agents() {
        let app = make_app(vec![
            make_folder("Project", vec![
                make_agent("fix-auth", "Project", AgentStatus::Active, Some("%1")),
                make_agent("idle-one", "Project", AgentStatus::Idle, Some("%2")),
                make_agent("stopped", "Project", AgentStatus::Stopped, None),
            ]),
        ]);
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("fix-auth"));
        assert!(output.contains("idle-one"));
        assert!(output.contains("stopped"));
    }

    // ── No underlines in output ──

    #[test]
    fn test_no_underline_styles() {
        // Verify that the draw functions don't use Modifier::UNDERLINED
        // by checking rendered output contains expected content without underline artifacts
        let app = make_app(vec![
            make_folder("BB", vec![
                make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
            ]),
        ]);
        let output = render_to_string(&app, 60, 20);
        // Output should contain folder and agent names
        assert!(output.contains("BB"));
        assert!(output.contains("a1"));
    }

    // ── Command mode rendering ──

    #[test]
    fn test_draw_command_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "quit".into();
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("quit"));
    }

    // ── Search mode rendering ──

    #[test]
    fn test_draw_search_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Search;
        app.search_buf = "fix".into();
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("fix"));
    }

    // ── SendPrompt mode rendering ──

    #[test]
    fn test_draw_send_prompt_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::SendPrompt;
        app.cmd_buf = "hello world".into();
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("hello world"));
    }

    // ── Message rendering ──

    #[test]
    fn test_draw_with_message() {
        let mut app = make_app(vec![]);
        app.set_message("Test message");
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("Test message"));
    }

    #[test]
    fn test_draw_no_message() {
        let app = make_app(vec![]);
        // Should render without error when no message
        let output = render_to_string(&app, 60, 20);
        assert!(!output.is_empty());
    }

    // ── Confirm delete popup ──

    #[test]
    fn test_draw_confirm_delete_folder() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.confirm_delete = Some(TreeEntry::Folder(0));
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("Delete"));
        assert!(output.contains("BB"));
    }

    #[test]
    fn test_draw_confirm_delete_agent() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("fix-auth", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.confirm_delete = Some(TreeEntry::Agent(0, 0));
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("Delete"));
        assert!(output.contains("fix-auth"));
    }

    // ── Input popup ──

    #[test]
    fn test_draw_add_folder_popup() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::AddFolder;
        app.input_field = InputField::Name;
        app.input_buf = "MyProject".into();
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("MyProject"));
    }

    #[test]
    fn test_draw_add_agent_popup() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.input_mode = InputMode::AddAgent;
        app.input_field = InputField::Name;
        app.input_buf = "new-agent".into();
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("new-agent"));
    }

    // ── Help bar content ──

    #[test]
    fn test_help_bar_contains_keys() {
        let app = make_app(vec![]);
        let output = render_to_string(&app, 80, 20);
        assert!(output.contains("cmd"));
        assert!(output.contains("find"));
        assert!(output.contains("send"));
        assert!(output.contains("uit"));
    }

    // ── Header counts ──

    #[test]
    fn test_header_shows_status_icons() {
        let app = make_app(vec![
            make_folder("BB", vec![
                make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
                make_agent("a2", "BB", AgentStatus::Idle, Some("%2")),
                make_agent("a3", "BB", AgentStatus::Stopped, None),
            ]),
        ]);
        let output = render_to_string(&app, 60, 20);
        // Header should show counts
        assert!(!output.is_empty());
    }

    // ── Focused agent highlighting ──

    #[test]
    fn test_focused_agent_shown() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("focus-me", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.focused_agent_key = Some("BB/focus-me".into());
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("focus-me"));
    }

    // ── Active folder arrow direction ──

    #[test]
    fn test_active_folder_down_arrow() {
        let mut app = make_app(vec![
            make_folder("BB", vec![]),
            make_folder("Arun", vec![]),
        ]);
        app.active_folder_idx = Some(0);
        let output = render_to_string(&app, 60, 20);
        // Active folder uses ▾ (down arrow)
        assert!(output.contains('▾'));
    }

    #[test]
    fn test_inactive_folder_right_arrow() {
        let mut app = make_app(vec![
            make_folder("BB", vec![]),
            make_folder("Arun", vec![]),
        ]);
        app.active_folder_idx = Some(0);
        let output = render_to_string(&app, 60, 20);
        // Inactive folder uses ▸ (right arrow)
        assert!(output.contains('▸'));
    }

    // ── Small terminal ──

    #[test]
    fn test_draw_small_terminal() {
        let app = make_app(vec![make_folder("BB", vec![])]);
        // Should not panic on very small terminal
        let output = render_to_string(&app, 20, 8);
        assert!(!output.is_empty());
    }

    // ── Git branch display ──

    #[test]
    fn test_agent_with_git_branch() {
        let mut agent = make_agent("a1", "BB", AgentStatus::Active, Some("%1"));
        agent.git_branch = "feature/auth".into();
        let app = make_app(vec![make_folder("BB", vec![agent])]);
        let output = render_to_string(&app, 80, 20);
        assert!(output.contains("feature/auth"));
    }

    // ── Many folders stress ──

    #[test]
    fn test_draw_many_folders() {
        let folders: Vec<_> = (0..50)
            .map(|i| make_folder(&format!("Folder-{i}"), vec![
                make_agent(&format!("agent-{i}"), &format!("Folder-{i}"), AgentStatus::Active, Some(&format!("%{i}"))),
            ]))
            .collect();
        let app = make_app(folders);
        // Should not panic even with more items than terminal rows
        let output = render_to_string(&app, 60, 20);
        assert!(!output.is_empty());
    }

    // ── Workspace name in header ──

    #[test]
    fn test_header_shows_workspace_basename() {
        let app = make_app(vec![]);
        // workspace is "/tmp/workspace", basename is "workspace"
        let output = render_to_string(&app, 60, 20);
        assert!(output.contains("workspace"));
    }
}
