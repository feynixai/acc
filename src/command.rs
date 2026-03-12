use crate::app::{App, InputMode, TreeEntry};
use crate::handler;
use crate::tmux;
use crossterm::event::KeyCode;

// ── : command mode ──

pub fn start_command(app: &mut App) {
    app.input_mode = InputMode::Command;
    app.cmd_buf.clear();
}

pub fn handle_command_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.cmd_buf.clear();
        }
        KeyCode::Backspace => {
            if app.cmd_buf.pop().is_none() {
                app.input_mode = InputMode::Normal;
            }
        }
        KeyCode::Char(c) => {
            app.cmd_buf.push(c);
        }
        KeyCode::Enter => {
            execute_command(app);
        }
        _ => {}
    }
}

fn execute_command(app: &mut App) {
    let cmd = app.cmd_buf.clone();
    app.cmd_buf.clear();
    app.input_mode = InputMode::Normal;

    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let (verb, arg) = (parts[0], parts.get(1).copied().unwrap_or(""));

    match verb {
        "q" | "quit" => {
            app.quit_requested = true;
        }
        "w" | "win" | "window" => {
            if !arg.is_empty() {
                jump_to_window(app, arg);
            } else {
                app.set_message("Usage: :w <number|name>");
            }
        }
        "p" | "pane" => {
            if !arg.is_empty() {
                jump_to_pane(app, arg);
            } else {
                app.set_message("Usage: :p <number|name>");
            }
        }
        "send" => {
            if !arg.is_empty() {
                send_to_focused(app, arg);
            } else {
                app.set_message("Usage: :send <text>");
            }
        }
        "stop" => {
            stop_focused(app);
        }
        "restart" => {
            restart_focused(app);
        }
        "rename" => {
            if !arg.is_empty() {
                rename_selected(app, arg);
            } else {
                app.set_message("Usage: :rename <name>");
            }
        }
        _ => {
            app.set_message(&format!("Unknown: :{verb}"));
        }
    }
}

fn jump_to_window(app: &mut App, arg: &str) {
    // Try as number first (1-based folder index)
    if let Ok(n) = arg.parse::<usize>() {
        let idx = n.saturating_sub(1);
        if idx < app.state.folders.len() {
            handler::switch_to_folder(app, idx);
            return;
        }
    }
    // Try as name prefix
    let lower = arg.to_lowercase();
    if let Some(idx) = app.state.folders.iter().position(|f| f.name.to_lowercase().starts_with(&lower)) {
        handler::switch_to_folder(app, idx);
    } else {
        app.set_message(&format!("No window: {arg}"));
    }
}

fn jump_to_pane(app: &mut App, arg: &str) {
    let Some(fi) = app.active_folder_idx else {
        app.set_message("No active folder");
        return;
    };
    if fi >= app.state.folders.len() { return; }

    // Try as number first (1-based agent index)
    if let Ok(n) = arg.parse::<usize>() {
        let idx = n.saturating_sub(1);
        if idx < app.state.folders[fi].agents.len() {
            handler::focus_agent(app, fi, idx);
            // Also select in tree
            let tree_items = app.tree_items();
            for (i, entry) in tree_items.iter().enumerate() {
                if matches!(entry, TreeEntry::Agent(f, a) if *f == fi && *a == idx) {
                    app.list_state.select(Some(i));
                    break;
                }
            }
            return;
        }
    }
    // Try as name prefix
    let lower = arg.to_lowercase();
    if let Some(idx) = app.state.folders[fi].agents.iter().position(|a| a.name.to_lowercase().starts_with(&lower)) {
        handler::focus_agent(app, fi, idx);
        let tree_items = app.tree_items();
        for (i, entry) in tree_items.iter().enumerate() {
            if matches!(entry, TreeEntry::Agent(f, a) if *f == fi && *a == idx) {
                app.list_state.select(Some(i));
                break;
            }
        }
    } else {
        app.set_message(&format!("No pane: {arg}"));
    }
}

fn find_focused_pane_id(app: &App) -> Option<(usize, usize)> {
    let key = app.focused_agent_key.as_ref()?;
    for (fi, folder) in app.state.folders.iter().enumerate() {
        for (ai, agent) in folder.agents.iter().enumerate() {
            if format!("{}/{}", agent.folder, agent.name) == *key {
                return Some((fi, ai));
            }
        }
    }
    None
}

fn send_to_focused(app: &mut App, text: &str) {
    if let Some((fi, ai)) = find_focused_pane_id(app) {
        if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
            tmux::send_to_pane(pane_id, text);
            let name = &app.state.folders[fi].agents[ai].name;
            app.set_message(&format!("Sent to {name}"));
        } else {
            app.set_message("Pane not running");
        }
    } else {
        app.set_message("No focused agent");
    }
}

fn stop_focused(app: &mut App) {
    if let Some((fi, ai)) = find_focused_pane_id(app) {
        if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
            tmux::stop_agent_process(pane_id);
            let name = app.state.folders[fi].agents[ai].name.clone();
            app.set_message(&format!("Stopped: {name}"));
        } else {
            app.set_message("Pane not running");
        }
    } else {
        app.set_message("No focused agent");
    }
}

fn restart_focused(app: &mut App) {
    if let Some((fi, ai)) = find_focused_pane_id(app) {
        let cmd = app.state.folders[fi].agents[ai].command.clone();
        if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
            tmux::restart_agent(pane_id, &cmd);
            let name = app.state.folders[fi].agents[ai].name.clone();
            app.set_message(&format!("Restarted: {name}"));
        } else {
            app.set_message("Pane not running");
        }
    } else {
        app.set_message("No focused agent");
    }
}

fn rename_selected(app: &mut App, name: &str) {
    let Some(entry) = app.selected_entry() else {
        app.set_message("Nothing selected");
        return;
    };
    match entry {
        TreeEntry::Agent(fi, ai) => {
            if fi < app.state.folders.len() && ai < app.state.folders[fi].agents.len() {
                if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
                    tmux::set_pane_title(pane_id, name);
                }
                app.state.folders[fi].agents[ai].name = name.to_string();
                app.set_message(&format!("Renamed: {name}"));
            }
        }
        TreeEntry::Folder(fi) => {
            if fi < app.state.folders.len() {
                let old = app.state.folders[fi].name.clone();
                app.state.folders[fi].name = name.to_string();
                if let Some(ref win) = app.state.folders[fi].window_index {
                    tmux::rename_window(&app.session, win, name);
                }
                crate::state::save_state(&app.state, &app.workspace);
                app.set_message(&format!("Renamed: {old} → {name}"));
            }
        }
    }
}

// ── / search mode ──

pub fn start_search(app: &mut App) {
    app.input_mode = InputMode::Search;
    app.search_buf.clear();
    app.search_matches.clear();
    app.search_idx = 0;
}

pub fn handle_search_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.search_buf.clear();
            app.search_matches.clear();
        }
        KeyCode::Backspace => {
            app.search_buf.pop();
            update_search_matches(app);
        }
        KeyCode::Char(c) => {
            app.search_buf.push(c);
            update_search_matches(app);
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            // Keep matches for ;/, cycling
            if !app.search_matches.is_empty() {
                let idx = app.search_matches[app.search_idx];
                app.list_state.select(Some(idx));
            }
        }
        _ => {}
    }
}

fn update_search_matches(app: &mut App) {
    let query = app.search_buf.to_lowercase();
    app.search_matches.clear();
    app.search_idx = 0;

    if query.is_empty() { return; }

    let items = app.tree_items();
    for (i, entry) in items.iter().enumerate() {
        let name = match entry {
            TreeEntry::Folder(fi) => {
                if *fi < app.state.folders.len() {
                    app.state.folders[*fi].name.to_lowercase()
                } else { continue; }
            }
            TreeEntry::Agent(fi, ai) => {
                if *fi < app.state.folders.len() && *ai < app.state.folders[*fi].agents.len() {
                    app.state.folders[*fi].agents[*ai].name.to_lowercase()
                } else { continue; }
            }
        };
        if name.contains(&query) {
            app.search_matches.push(i);
        }
    }

    // Jump to first match
    if !app.search_matches.is_empty() {
        app.list_state.select(Some(app.search_matches[0]));
    }
}

pub fn next_search_match(app: &mut App) {
    if app.search_matches.is_empty() { return; }
    app.search_idx = (app.search_idx + 1) % app.search_matches.len();
    let idx = app.search_matches[app.search_idx];
    app.list_state.select(Some(idx));
}

pub fn prev_search_match(app: &mut App) {
    if app.search_matches.is_empty() { return; }
    app.search_idx = if app.search_idx == 0 { app.search_matches.len() - 1 } else { app.search_idx - 1 };
    let idx = app.search_matches[app.search_idx];
    app.list_state.select(Some(idx));
}

// ── > send prompt mode ──

pub fn start_send_prompt(app: &mut App) {
    app.input_mode = InputMode::SendPrompt;
    app.cmd_buf.clear();
}

pub fn handle_send_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.cmd_buf.clear();
        }
        KeyCode::Backspace => {
            if app.cmd_buf.pop().is_none() {
                app.input_mode = InputMode::Normal;
            }
        }
        KeyCode::Char(c) => {
            app.cmd_buf.push(c);
        }
        KeyCode::Enter => {
            let text = app.cmd_buf.clone();
            app.cmd_buf.clear();
            app.input_mode = InputMode::Normal;
            if !text.is_empty() {
                send_to_focused(app, &text);
            }
        }
        _ => {}
    }
}

// ── 1-9 quick jump ──

pub fn jump_to_pane_number(app: &mut App, n: usize) {
    let Some(fi) = app.active_folder_idx else { return };
    if fi >= app.state.folders.len() { return; }
    let idx = n.saturating_sub(1);
    if idx < app.state.folders[fi].agents.len() {
        handler::focus_agent(app, fi, idx);
        // Also select in tree
        let tree_items = app.tree_items();
        for (i, entry) in tree_items.iter().enumerate() {
            if matches!(entry, TreeEntry::Agent(f, a) if *f == fi && *a == idx) {
                app.list_state.select(Some(i));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{make_app, make_agent, make_folder};
    use crate::state::AgentStatus;
    use crossterm::event::KeyCode;

    // ── Command mode transitions ──

    #[test]
    fn test_start_command_sets_mode() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        assert_eq!(app.input_mode, InputMode::Command);
        assert!(app.cmd_buf.is_empty());
    }

    #[test]
    fn test_command_esc_returns_normal() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        handle_command_key(&mut app, KeyCode::Esc);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.cmd_buf.is_empty());
    }

    #[test]
    fn test_command_typing() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        handle_command_key(&mut app, KeyCode::Char('q'));
        assert_eq!(app.cmd_buf, "q");
        handle_command_key(&mut app, KeyCode::Char('u'));
        assert_eq!(app.cmd_buf, "qu");
    }

    #[test]
    fn test_command_backspace() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        handle_command_key(&mut app, KeyCode::Char('a'));
        handle_command_key(&mut app, KeyCode::Char('b'));
        handle_command_key(&mut app, KeyCode::Backspace);
        assert_eq!(app.cmd_buf, "a");
    }

    #[test]
    fn test_command_backspace_empty_exits() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        handle_command_key(&mut app, KeyCode::Backspace);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_command_enter_executes() {
        let mut app = make_app(vec![]);
        start_command(&mut app);
        handle_command_key(&mut app, KeyCode::Char('q'));
        handle_command_key(&mut app, KeyCode::Enter);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.quit_requested);
    }

    // ── execute_command: quit ──

    #[test]
    fn test_execute_quit() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "q".into();
        execute_command(&mut app);
        assert!(app.quit_requested);
    }

    #[test]
    fn test_execute_quit_long() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "quit".into();
        execute_command(&mut app);
        assert!(app.quit_requested);
    }

    // ── execute_command: unknown ──

    #[test]
    fn test_execute_unknown_command() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "foobar".into();
        execute_command(&mut app);
        assert!(!app.quit_requested);
        assert_eq!(app.message.as_deref(), Some("Unknown: :foobar"));
    }

    // ── execute_command: window/pane without args ──

    #[test]
    fn test_window_no_arg_shows_usage() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "w".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("Usage: :w <number|name>"));
    }

    #[test]
    fn test_pane_no_arg_shows_usage() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "p".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("Usage: :p <number|name>"));
    }

    #[test]
    fn test_send_no_arg_shows_usage() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "send".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("Usage: :send <text>"));
    }

    #[test]
    fn test_rename_no_arg_shows_usage() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "rename".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("Usage: :rename <name>"));
    }

    // ── execute_command: stop/restart with no focus ──

    #[test]
    fn test_stop_no_focused_agent() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.focused_agent_key = None;
        app.input_mode = InputMode::Command;
        app.cmd_buf = "stop".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("No focused agent"));
    }

    #[test]
    fn test_restart_no_focused_agent() {
        let mut app = make_app(vec![]);
        app.focused_agent_key = None;
        app.input_mode = InputMode::Command;
        app.cmd_buf = "restart".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("No focused agent"));
    }

    // ── find_focused_pane_id ──

    #[test]
    fn test_find_focused_pane_id_found() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
            make_agent("claude-11", "BB", AgentStatus::Idle, Some("%11")),
        ])]);
        app.focused_agent_key = Some("BB/claude-11".into());
        let result = find_focused_pane_id(&app);
        assert_eq!(result, Some((0, 1)));
    }

    #[test]
    fn test_find_focused_pane_id_not_found() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
        ])]);
        app.focused_agent_key = Some("BB/nonexistent".into());
        assert_eq!(find_focused_pane_id(&app), None);
    }

    #[test]
    fn test_find_focused_pane_id_none_key() {
        let app = make_app(vec![make_folder("BB", vec![
            make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
        ])]);
        assert_eq!(find_focused_pane_id(&app), None);
    }

    // ── send_to_focused edge cases ──

    #[test]
    fn test_send_to_focused_no_agent() {
        let mut app = make_app(vec![]);
        app.focused_agent_key = None;
        send_to_focused(&mut app, "hello");
        assert_eq!(app.message.as_deref(), Some("No focused agent"));
    }

    #[test]
    fn test_send_to_focused_no_pane() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Stopped, None),
        ])]);
        app.focused_agent_key = Some("BB/a1".into());
        send_to_focused(&mut app, "hello");
        assert_eq!(app.message.as_deref(), Some("Pane not running"));
    }

    // ── jump_to_window ──

    #[test]
    fn test_jump_to_window_not_found() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        jump_to_window(&mut app, "nonexistent");
        assert_eq!(app.message.as_deref(), Some("No window: nonexistent"));
    }

    #[test]
    fn test_jump_to_window_number_out_of_range() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        jump_to_window(&mut app, "99");
        assert_eq!(app.message.as_deref(), Some("No window: 99"));
    }

    // ── jump_to_pane ──

    #[test]
    fn test_jump_to_pane_no_active_folder() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.active_folder_idx = None;
        jump_to_pane(&mut app, "1");
        assert_eq!(app.message.as_deref(), Some("No active folder"));
    }

    #[test]
    fn test_jump_to_pane_not_found() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.active_folder_idx = Some(0);
        jump_to_pane(&mut app, "nonexistent");
        assert_eq!(app.message.as_deref(), Some("No pane: nonexistent"));
    }

    // ── Search mode ──

    #[test]
    fn test_start_search_sets_mode() {
        let mut app = make_app(vec![]);
        start_search(&mut app);
        assert_eq!(app.input_mode, InputMode::Search);
        assert!(app.search_buf.is_empty());
        assert!(app.search_matches.is_empty());
    }

    #[test]
    fn test_search_esc_returns_normal() {
        let mut app = make_app(vec![]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Esc);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.search_buf.is_empty());
    }

    #[test]
    fn test_search_typing_updates_matches() {
        let mut app = make_app(vec![
            make_folder("Alpha", vec![
                make_agent("fix-auth", "Alpha", AgentStatus::Active, Some("%1")),
            ]),
            make_folder("Beta", vec![
                make_agent("fix-bug", "Beta", AgentStatus::Active, Some("%2")),
            ]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('f'));
        handle_search_key(&mut app, KeyCode::Char('i'));
        handle_search_key(&mut app, KeyCode::Char('x'));
        assert_eq!(app.search_buf, "fix");
        // Should match "fix-auth" (index 1) and "fix-bug" (index 3)
        assert_eq!(app.search_matches.len(), 2);
    }

    #[test]
    fn test_search_matches_folders() {
        let mut app = make_app(vec![
            make_folder("Alpha", vec![]),
            make_folder("Bravo", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('a'));
        handle_search_key(&mut app, KeyCode::Char('l'));
        // "alpha" matches "Alpha" (case insensitive)
        assert_eq!(app.search_matches.len(), 1);
        assert_eq!(app.search_matches[0], 0);
    }

    #[test]
    fn test_search_backspace_recalculates() {
        let mut app = make_app(vec![
            make_folder("Alpha", vec![]),
            make_folder("Bravo", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('a'));
        handle_search_key(&mut app, KeyCode::Char('l'));
        assert_eq!(app.search_matches.len(), 1);
        handle_search_key(&mut app, KeyCode::Backspace);
        // Now query is "a" — matches "Alpha" and "Bravo" (has 'a')
        assert!(app.search_matches.len() >= 1);
    }

    #[test]
    fn test_search_enter_keeps_matches() {
        let mut app = make_app(vec![
            make_folder("Alpha", vec![]),
            make_folder("Bravo", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('b'));
        assert!(!app.search_matches.is_empty());
        let matches_before = app.search_matches.clone();
        handle_search_key(&mut app, KeyCode::Enter);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.search_matches, matches_before);
    }

    #[test]
    fn test_search_no_matches() {
        let mut app = make_app(vec![
            make_folder("Alpha", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('z'));
        handle_search_key(&mut app, KeyCode::Char('z'));
        assert!(app.search_matches.is_empty());
    }

    // ── Search cycling ──

    #[test]
    fn test_next_search_match_cycles() {
        let mut app = make_app(vec![
            make_folder("Aaa", vec![]),
            make_folder("Aab", vec![]),
            make_folder("Aac", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('a'));
        assert_eq!(app.search_matches.len(), 3);
        assert_eq!(app.search_idx, 0);

        next_search_match(&mut app);
        assert_eq!(app.search_idx, 1);
        next_search_match(&mut app);
        assert_eq!(app.search_idx, 2);
        next_search_match(&mut app);
        assert_eq!(app.search_idx, 0); // wraps
    }

    #[test]
    fn test_prev_search_match_cycles() {
        let mut app = make_app(vec![
            make_folder("Aaa", vec![]),
            make_folder("Aab", vec![]),
            make_folder("Aac", vec![]),
        ]);
        start_search(&mut app);
        handle_search_key(&mut app, KeyCode::Char('a'));
        assert_eq!(app.search_idx, 0);

        prev_search_match(&mut app);
        assert_eq!(app.search_idx, 2); // wraps backward
        prev_search_match(&mut app);
        assert_eq!(app.search_idx, 1);
    }

    #[test]
    fn test_next_search_match_empty() {
        let mut app = make_app(vec![]);
        next_search_match(&mut app);
        assert_eq!(app.search_idx, 0);
    }

    #[test]
    fn test_prev_search_match_empty() {
        let mut app = make_app(vec![]);
        prev_search_match(&mut app);
        assert_eq!(app.search_idx, 0);
    }

    // ── SendPrompt mode ──

    #[test]
    fn test_start_send_prompt_sets_mode() {
        let mut app = make_app(vec![]);
        start_send_prompt(&mut app);
        assert_eq!(app.input_mode, InputMode::SendPrompt);
        assert!(app.cmd_buf.is_empty());
    }

    #[test]
    fn test_send_prompt_esc() {
        let mut app = make_app(vec![]);
        start_send_prompt(&mut app);
        handle_send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_send_prompt_typing() {
        let mut app = make_app(vec![]);
        start_send_prompt(&mut app);
        handle_send_key(&mut app, KeyCode::Char('h'));
        handle_send_key(&mut app, KeyCode::Char('i'));
        assert_eq!(app.cmd_buf, "hi");
    }

    #[test]
    fn test_send_prompt_backspace_empty_exits() {
        let mut app = make_app(vec![]);
        start_send_prompt(&mut app);
        handle_send_key(&mut app, KeyCode::Backspace);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_send_prompt_enter_clears() {
        let mut app = make_app(vec![]);
        start_send_prompt(&mut app);
        handle_send_key(&mut app, KeyCode::Char('t'));
        handle_send_key(&mut app, KeyCode::Enter);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.cmd_buf.is_empty());
    }

    // ── Command aliases ──

    #[test]
    fn test_command_aliases_window() {
        for alias in &["w", "win", "window"] {
            let mut app = make_app(vec![make_folder("BB", vec![])]);
            app.input_mode = InputMode::Command;
            app.cmd_buf = format!("{} 1", alias);
            execute_command(&mut app);
            // Should not show "Unknown" error
            assert_ne!(app.message.as_deref(), Some(&format!("Unknown: :{}", alias)[..]));
        }
    }

    #[test]
    fn test_command_aliases_pane() {
        for alias in &["p", "pane"] {
            let mut app = make_app(vec![make_folder("BB", vec![
                make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
            ])]);
            app.active_folder_idx = Some(0);
            app.input_mode = InputMode::Command;
            app.cmd_buf = format!("{} 1", alias);
            execute_command(&mut app);
            assert_ne!(app.message.as_deref(), Some(&format!("Unknown: :{}", alias)[..]));
        }
    }

    // ── jump_to_pane_number ──

    #[test]
    fn test_jump_to_pane_number_no_active_folder() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.active_folder_idx = None;
        jump_to_pane_number(&mut app, 1);
        // Should not panic, no selection change
    }

    #[test]
    fn test_jump_to_pane_number_out_of_range() {
        let mut app = make_app(vec![make_folder("BB", vec![
            make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
        ])]);
        app.active_folder_idx = Some(0);
        let sel_before = app.list_state.selected();
        jump_to_pane_number(&mut app, 99);
        // Selection shouldn't change for out-of-range
        assert_eq!(app.list_state.selected(), sel_before);
    }

    // ── execute_command clears state ──

    #[test]
    fn test_execute_command_clears_buf_and_mode() {
        let mut app = make_app(vec![]);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "unknown_cmd".into();
        execute_command(&mut app);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.cmd_buf.is_empty());
    }

    // ── rename with nothing selected ──

    #[test]
    fn test_rename_nothing_selected() {
        let mut app = make_app(vec![]);
        app.list_state.select(None);
        app.input_mode = InputMode::Command;
        app.cmd_buf = "rename foo".into();
        execute_command(&mut app);
        assert_eq!(app.message.as_deref(), Some("Nothing selected"));
    }
}
