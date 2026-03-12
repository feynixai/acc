use crate::app::{App, InputMode, InputField, TreeEntry, UndoEntry};
use crate::state::save_state;
use crate::tmux;
use std::{fs, path::Path};

// ── Core: ensure we always have a folder for the current window ──

pub fn ensure_default_folder(app: &mut App) {
    let current_win = tmux::current_window();

    // Check if any folder already owns this window
    for (i, f) in app.state.folders.iter().enumerate() {
        if f.window_index.as_deref() == Some(&current_win) {
            app.active_folder_idx = Some(i);
            sync_current_window(app);
            return;
        }
    }

    // Try to adopt an orphan folder (one that has no window assigned yet).
    let orphan_idx = app.state.folders.iter().position(|f| f.window_index.is_none());
    if let Some(i) = orphan_idx {
        app.state.folders[i].window_index = Some(current_win.clone());
        tmux::rename_window(&app.session, &current_win, &app.state.folders[i].name);
        app.active_folder_idx = Some(i);
        app.list_state.select(Some(0));
        sync_current_window(app);
        save_state(&app.state, &app.workspace);
        return;
    }

    // No orphan — create a brand new folder
    let ws_name = std::path::Path::new(&app.workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());

    let name = if app.state.get_folder(&ws_name).is_some() {
        format!("{}-{}", ws_name, current_win)
    } else {
        ws_name.clone()
    };

    app.state.add_folder(name.clone(), app.workspace.clone());
    if let Some(f) = app.state.get_folder_mut(&name) {
        f.window_index = Some(current_win.clone());
    }
    tmux::rename_window(&app.session, &current_win, &name);
    app.active_folder_idx = app.state.folders.iter().position(|f| f.name == name);
    app.list_state.select(Some(0));
    sync_current_window(app);
    save_state(&app.state, &app.workspace);
}

// ── Sync: rebuild agents from tmux panes ──

/// Sync ALL folders on each tick so non-active folders also show their agents.
pub fn sync_all_folders(app: &mut App) {
    let active_fi = app.active_folder_idx;

    // Update active folder's window_index to current
    if let Some(fi) = active_fi {
        if fi < app.state.folders.len() {
            let current_win = tmux::current_window();
            app.state.folders[fi].window_index = Some(current_win);
        }
    }

    // Sync every folder that has a window
    let session = app.session.clone();
    for (i, folder) in app.state.folders.iter_mut().enumerate() {
        if folder.window_index.is_some() {
            let fetch_git = Some(i) == active_fi;
            tmux::sync_folder_panes(folder, &session, fetch_git);
        }
    }
}

/// Sync only the active folder (used after explicit operations).
pub fn sync_current_window(app: &mut App) {
    let Some(fi) = app.active_folder_idx else { return };
    if fi >= app.state.folders.len() { return; }

    let current_win = tmux::current_window();
    app.state.folders[fi].window_index = Some(current_win);

    tmux::sync_folder_panes(&mut app.state.folders[fi], &app.session, true);
}

// ── Enter: folder → switch window, agent → focus pane ──

pub fn handle_enter(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    match entry {
        TreeEntry::Folder(fi) => switch_to_folder(app, fi),
        TreeEntry::Agent(fi, ai) => focus_agent(app, fi, ai),
    }
}

pub fn switch_to_folder(app: &mut App, fi: usize) {
    if fi >= app.state.folders.len() { return; }

    let win_idx = app.state.folders[fi].window_index.clone();
    let Some(ref target_win) = win_idx else {
        app.set_message("No window for this folder");
        return;
    };

    let sidebar = app.sidebar_pane_id.clone();
    let Some(ref sidebar_pane) = sidebar else {
        app.set_message("Sidebar pane not found");
        return;
    };

    tmux::switch_to_folder_window(sidebar_pane, target_win, &app.session);
    app.active_folder_idx = Some(fi);

    // Re-detect sidebar pane ID (search all session panes)
    app.sidebar_pane_id = tmux::sidebar_pane_id();

    // Refresh ALL folder window indices — they may have shifted
    tmux::refresh_folder_windows(&mut app.state.folders, &app.session);

    sync_current_window(app);
    save_state(&app.state, &app.workspace);
}

pub fn focus_agent(app: &mut App, fi: usize, ai: usize) {
    // If agent is in a different folder, switch to that folder first
    if app.active_folder_idx != Some(fi) {
        switch_to_folder(app, fi);
    }

    if fi >= app.state.folders.len() { return; }
    if ai >= app.state.folders[fi].agents.len() { return; }

    let agent = &app.state.folders[fi].agents[ai];
    let target_pane_id = agent.pane_id.clone();
    let is_hidden = agent.hidden;
    let agent_key = format!("{}/{}", agent.folder, agent.name);
    let folder_name = app.state.folders[fi].name.clone();
    let session = app.session.clone();

    let Some(ref target_pid) = target_pane_id else {
        app.set_message("Pane not running");
        return;
    };

    // If selected pane is hidden, bring it back first
    if is_hidden {
        // Join back next to sidebar
        if let Some(ref sidebar_id) = app.sidebar_pane_id {
            tmux::join_pane_back(target_pid, sidebar_id);
        } else if let Some(ref win_idx) = app.state.folders[fi].window_index {
            let target = format!("{}:{}", session, win_idx);
            tmux::join_pane_back(target_pid, &target);
        }
    }

    // Hide all OTHER visible (non-hidden) agent panes to background
    let other_panes: Vec<String> = app.state.folders[fi].agents.iter()
        .filter(|a| {
            a.pane_id.as_deref() != Some(target_pid)
                && !a.hidden
                && a.pane_id.is_some()
        })
        .filter_map(|a| a.pane_id.clone())
        .collect();

    for pid in &other_panes {
        tmux::break_pane_to_background(pid, &session, &folder_name);
    }

    tmux::select_pane(target_pid);
    app.focused_agent_key = Some(agent_key);

    // Re-enforce sidebar width — pane joins/breaks cause tmux to shrink it
    if let Some(ref sidebar_id) = app.sidebar_pane_id {
        tmux::resize_sidebar(sidebar_id);
    }

    sync_current_window(app);
}

/// Hide the selected pane to background (c key — VS Code close-tab behavior).
/// Process keeps running, pane just disappears from the split layout.
pub fn hide_pane(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    match entry {
        TreeEntry::Agent(fi, ai) => {
            if fi >= app.state.folders.len() { return; }
            if ai >= app.state.folders[fi].agents.len() { return; }
            let agent = &app.state.folders[fi].agents[ai];
            if agent.hidden {
                app.set_message("Already hidden");
                return;
            }
            let name = agent.name.clone();
            let folder_name = app.state.folders[fi].name.clone();
            if let Some(ref pane_id) = agent.pane_id {
                tmux::break_pane_to_background(pane_id, &app.session, &folder_name);
                if let Some(ref sidebar_id) = app.sidebar_pane_id {
                    tmux::resize_sidebar(sidebar_id);
                }
                app.set_message(&format!("Hidden: {name}"));
                sync_current_window(app);
            } else {
                app.set_message("Pane not running");
            }
        }
        TreeEntry::Folder(_) => {
            app.set_message("Select an agent to hide");
        }
    }
}

// ── Split: h = horizontal (top/bottom), v = vertical (left/right) ──

/// h/v: Show the selected agent in a split alongside the currently focused agent.
/// - If selected agent is hidden (background), join it back as a split.
/// - If selected agent is the focused one, create a new shell split.
/// - Direction: 'h' = horizontal (top/bottom), 'v' = vertical (left/right).
pub fn split_agent(app: &mut App, direction: char) {
    let Some(entry) = app.selected_entry() else { return };
    let (fi, ai) = match entry {
        TreeEntry::Agent(fi, ai) => (fi, ai),
        TreeEntry::Folder(_) => {
            app.set_message("Select an agent to split");
            return;
        }
    };

    if fi >= app.state.folders.len() || ai >= app.state.folders[fi].agents.len() { return; }

    let agent = &app.state.folders[fi].agents[ai];
    let Some(ref selected_pane) = agent.pane_id else {
        app.set_message("Pane not running");
        return;
    };
    let selected_pane = selected_pane.clone();
    let selected_name = agent.name.clone();
    let is_hidden = agent.hidden;
    let folder_name = app.state.folders[fi].name.clone();
    let session = app.session.clone();

    // Find the currently visible (focused) agent's pane as the split target
    let focused_pane = app.focused_agent_key.as_ref().and_then(|key| {
        app.state.folders[fi].agents.iter()
            .find(|a| {
                let k = format!("{}/{}", a.folder, a.name);
                k == *key && !a.hidden && a.pane_id.is_some()
            })
            .and_then(|a| a.pane_id.clone())
    });

    let target_pane = if let Some(ref fp) = focused_pane {
        fp.clone()
    } else {
        // No focused agent — use the selected agent itself
        selected_pane.clone()
    };

    if is_hidden {
        // Selected agent is in background — join it back as a split.
        // -b = place before target (left/top), so new pane gets index 1.
        let join_flag = match direction {
            'h' => "-v", // horizontal split = tmux vertical (top/bottom)
            _ => "-h",   // vertical split = tmux horizontal (left/right)
        };
        tmux::tmux_cmd(&[
            "join-pane", join_flag, "-b",
            "-s", &selected_pane,
            "-t", &target_pane,
        ]);
        app.set_message(&format!("Split: {selected_name}"));
    } else if selected_pane == target_pane {
        // Same pane — create a new shell split before it (left/top)
        let cwd = agent.working_dir.clone();
        let new_pane = match direction {
            'h' => tmux::split_h_before(&target_pane, "", &cwd),
            'v' => tmux::split_v_before(&target_pane, "", &cwd),
            _ => None,
        };
        if let Some(ref new_id) = new_pane {
            let short_id = new_id.trim_start_matches('%');
            let name = format!("pane-{short_id}");
            tmux::set_pane_title(new_id, &name);
            app.set_message(&format!("Split: {name}"));
        } else {
            app.set_message("Split failed");
        }
    } else {
        // Selected agent is visible but not focused — hide it first, then join as split
        tmux::break_pane_to_background(&selected_pane, &session, &folder_name);
        let join_flag = match direction {
            'h' => "-v",
            _ => "-h",
        };
        tmux::tmux_cmd(&[
            "join-pane", join_flag, "-b",
            "-s", &selected_pane,
            "-t", &target_pane,
        ]);
        app.set_message(&format!("Split: {selected_name}"));
    }

    if let Some(ref sidebar_id) = app.sidebar_pane_id {
        tmux::resize_sidebar(sidebar_id);
    }
    sync_current_window(app);
}

// ── Close pane (d key) ──

pub fn close_selected_pane(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    match entry {
        TreeEntry::Agent(fi, ai) => {
            if fi >= app.state.folders.len() { return; }
            if ai >= app.state.folders[fi].agents.len() { return; }
            let name = app.state.folders[fi].agents[ai].name.clone();
            if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
                tmux::kill_pane(pane_id);
                app.set_message(&format!("Closed: {name}"));
            }
        }
        TreeEntry::Folder(_) => {
            app.set_message("Use 'x' to delete folders");
        }
    }
}

// ── Stop / Remove ──

pub fn stop_selected(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    if let TreeEntry::Agent(fi, ai) = entry {
        if fi >= app.state.folders.len() { return; }
        if ai >= app.state.folders[fi].agents.len() { return; }
        if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
            tmux::stop_agent_process(pane_id);
        }
        let name = app.state.folders[fi].agents[ai].name.clone();
        app.set_message(&format!("Stopped: {name}"));
    }
}

pub fn remove_selected(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    app.confirm_delete = Some(entry);
}

pub fn execute_delete(app: &mut App) {
    let Some(entry) = app.confirm_delete.take() else { return };
    match entry {
        TreeEntry::Folder(fi) => {
            if fi >= app.state.folders.len() { return; }
            let folder = &app.state.folders[fi];
            let name = folder.name.clone();
            let path = folder.path.clone();

            // Save undo entry before destroying
            app.undo_stack.push(UndoEntry::Folder { name: name.clone(), path });

            if let Some(ref win) = app.state.folders[fi].window_index {
                let current_win = tmux::current_window();
                if *win != current_win {
                    tmux::kill_window(&app.session, win);
                }
            }
            app.state.remove_folder(&name);
            if app.active_folder_idx == Some(fi) {
                app.active_folder_idx = None;
                ensure_default_folder(app);
            }
            save_state(&app.state, &app.workspace);
            app.list_state.select(Some(0));
            app.set_message(&format!("Removed: {name} (u to undo)"));
        }
        TreeEntry::Agent(fi, ai) => {
            if fi >= app.state.folders.len() { return; }
            if ai >= app.state.folders[fi].agents.len() { return; }
            let agent = &app.state.folders[fi].agents[ai];
            let aname = agent.name.clone();
            let folder_name = app.state.folders[fi].name.clone();

            // Save undo entry before destroying
            app.undo_stack.push(UndoEntry::Agent {
                folder_name,
                agent_name: aname.clone(),
                command: agent.command.clone(),
                working_dir: agent.working_dir.clone(),
            });

            if let Some(ref pane_id) = agent.pane_id {
                tmux::kill_pane(pane_id);
            }
            app.set_message(&format!("Removed: {aname} (u to undo)"));
        }
    }
}

// ── Restart selected agent ──

pub fn restart_selected(app: &mut App) {
    let Some(entry) = app.selected_entry() else { return };
    if let TreeEntry::Agent(fi, ai) = entry {
        if fi >= app.state.folders.len() { return; }
        if ai >= app.state.folders[fi].agents.len() { return; }
        let agent = &app.state.folders[fi].agents[ai];
        let name = agent.name.clone();
        if let Some(ref pane_id) = agent.pane_id {
            let cmd = &agent.command;
            tmux::restart_agent(pane_id, cmd);
            app.set_message(&format!("Restarted: {name}"));
        } else {
            app.set_message("Pane not running");
        }
    } else {
        app.set_message("Select an agent to restart");
    }
}

// ── Undo last delete ──

pub fn undo_last(app: &mut App) {
    let Some(entry) = app.undo_stack.pop() else {
        app.set_message("Nothing to undo");
        return;
    };
    match entry {
        UndoEntry::Agent { folder_name, agent_name, command, working_dir } => {
            // Find the folder
            let fi = app.state.folders.iter().position(|f| f.name == folder_name);
            let Some(fi) = fi else {
                app.set_message(&format!("Folder '{folder_name}' no longer exists"));
                return;
            };

            // Find a pane to split from
            let target_pane = app.state.folders[fi].agents.iter()
                .find_map(|a| a.pane_id.clone());
            let Some(target) = target_pane else {
                app.set_message("No pane to split from");
                return;
            };

            // Recreate: split a new pane, run the command, set the title
            let cmd_str = if command == "zsh" || command == "bash" || command == "fish" || command == "sh" {
                String::new() // shell panes don't need a command sent
            } else {
                command
            };
            if let Some(new_id) = tmux::split_v(&target, &cmd_str, &working_dir) {
                tmux::set_pane_title(&new_id, &agent_name);
                app.set_message(&format!("Restored: {agent_name}"));
            } else {
                app.set_message("Failed to restore pane");
            }
        }
        UndoEntry::Folder { name, path } => {
            let session = app.session.clone();
            if let Some(win_idx) = tmux::create_folder_window(&session, &name, &path) {
                app.state.add_folder(name.clone(), path);
                if let Some(f) = app.state.get_folder_mut(&name) {
                    f.window_index = Some(win_idx);
                }
                save_state(&app.state, &app.workspace);
                app.set_message(&format!("Restored: {name}"));
            } else {
                app.set_message("Failed to restore folder");
            }
        }
    }
}

// ── Rename (R key) ──

pub fn start_rename(app: &mut App) {
    let Some(entry) = app.selected_entry() else {
        app.set_message("Nothing selected");
        return;
    };
    // Pre-fill with current name
    let name = match entry {
        TreeEntry::Folder(fi) => {
            if fi < app.state.folders.len() { app.state.folders[fi].name.clone() }
            else { return; }
        }
        TreeEntry::Agent(fi, ai) => {
            if fi < app.state.folders.len() && ai < app.state.folders[fi].agents.len() {
                app.state.folders[fi].agents[ai].name.clone()
            } else { return; }
        }
    };
    app.input_mode = InputMode::Rename;
    app.input_buf = name;
}

pub fn handle_rename_key(app: &mut App, key: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode;
    match key {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.input_buf.clear();
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c) => { app.input_buf.push(c); }
        KeyCode::Enter => {
            let new_name = app.input_buf.clone();
            app.input_buf.clear();
            app.input_mode = InputMode::Normal;
            if new_name.is_empty() { return; }
            commit_rename(app, &new_name);
        }
        _ => {}
    }
}

fn commit_rename(app: &mut App, name: &str) {
    let Some(entry) = app.selected_entry() else { return; };
    match entry {
        TreeEntry::Agent(fi, ai) => {
            if fi < app.state.folders.len() && ai < app.state.folders[fi].agents.len() {
                if let Some(ref pane_id) = app.state.folders[fi].agents[ai].pane_id {
                    crate::tmux::set_pane_title(pane_id, name);
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
                    crate::tmux::rename_window(&app.session, win, name);
                }
                save_state(&app.state, &app.workspace);
                app.set_message(&format!("Renamed: {old} → {name}"));
            }
        }
    }
}

// ── Focus detection ──

pub fn update_sidebar_focus(app: &mut App) {
    if let Some(ref sidebar_id) = app.sidebar_pane_id {
        let active_pane = crate::tmux::current_pane();
        app.sidebar_focused = active_pane == *sidebar_id;
    }
}

// ── Navigation ──

pub fn move_selection(app: &mut App, delta: i32) {
    let items = app.tree_items();
    if items.is_empty() { return; }
    let current = app.list_state.selected().unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, items.len() as i32 - 1) as usize;
    app.list_state.select(Some(next));
}

pub fn prev_window(app: &mut App) {
    let folders = &app.state.folders;
    if folders.len() < 2 { return; }
    let current = app.active_folder_idx.unwrap_or(0);
    let prev = if current == 0 { folders.len() - 1 } else { current - 1 };
    switch_to_folder(app, prev);
}

pub fn next_window(app: &mut App) {
    let folders = &app.state.folders;
    if folders.len() < 2 { return; }
    let current = app.active_folder_idx.unwrap_or(0);
    let next = if current >= folders.len() - 1 { 0 } else { current + 1 };
    switch_to_folder(app, next);
}

pub fn next_pane(app: &mut App) {
    let Some(fi) = app.active_folder_idx else { return };
    if fi >= app.state.folders.len() { return; }
    let agents = &app.state.folders[fi].agents;
    if agents.is_empty() { return; }

    let current_idx = app.focused_agent_key.as_ref().and_then(|key| {
        agents.iter().position(|a| format!("{}/{}", a.folder, a.name) == *key)
    }).unwrap_or(0);

    let next = if current_idx >= agents.len() - 1 { 0 } else { current_idx + 1 };
    focus_agent(app, fi, next);

    let tree_items = app.tree_items();
    for (i, entry) in tree_items.iter().enumerate() {
        if matches!(entry, TreeEntry::Agent(f, a) if *f == fi && *a == next) {
            app.list_state.select(Some(i));
            break;
        }
    }
}

pub fn prev_pane(app: &mut App) {
    let Some(fi) = app.active_folder_idx else { return };
    if fi >= app.state.folders.len() { return; }
    let agents = &app.state.folders[fi].agents;
    if agents.is_empty() { return; }

    let current_idx = app.focused_agent_key.as_ref().and_then(|key| {
        agents.iter().position(|a| format!("{}/{}", a.folder, a.name) == *key)
    }).unwrap_or(0);

    let prev = if current_idx == 0 { agents.len() - 1 } else { current_idx - 1 };
    focus_agent(app, fi, prev);

    let tree_items = app.tree_items();
    for (i, entry) in tree_items.iter().enumerate() {
        if matches!(entry, TreeEntry::Agent(f, a) if *f == fi && *a == prev) {
            app.list_state.select(Some(i));
            break;
        }
    }
}

pub fn handle_click(app: &mut App, _col: u16, row: u16, size: ratatui::layout::Size) {
    let height = size.height;
    // Layout: header(1) + tree(variable) + status(1)
    let tree_start = 1u16;
    let tree_end = height.saturating_sub(1);

    // Tree area: single click = select, double click = activate
    if row >= tree_start && row < tree_end {
        // Account for list scroll offset
        let scroll_offset = app.list_state.offset();
        let idx = (row - tree_start) as usize + scroll_offset;
        let items = app.tree_items();
        if idx < items.len() {
            let now = std::time::Instant::now();
            let is_double = app.last_click_row == Some(row)
                && now.duration_since(app.last_click_time).as_millis() < 400;

            app.list_state.select(Some(idx));
            app.last_click_time = now;
            app.last_click_row = Some(row);

            if is_double {
                handle_enter(app);
            }
        }
    }
}

// ── Input ──

pub fn start_add_folder(app: &mut App) {
    app.input_mode = InputMode::AddFolder;
    app.input_field = InputField::Name;
    app.input_buf.clear();
    app.input_stage = 0;
}

pub fn start_add_agent(app: &mut App) {
    if app.selected_folder_index().is_none() {
        app.set_message("Select a folder first");
        return;
    }
    app.input_mode = InputMode::AddAgent;
    app.input_field = InputField::Name;
    app.input_buf.clear();
    app.input_stage = 0;
}

pub fn handle_input_key(app: &mut App, key: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode;
    match key {
        KeyCode::Esc => { app.input_mode = InputMode::Normal; app.input_buf.clear(); }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Tab => tab_complete_path(app),
        KeyCode::Char(c) => { app.input_buf.push(c); }
        KeyCode::Enter => commit_input(app),
        _ => {}
    }
}

pub fn tab_complete_path(app: &mut App) {
    if app.input_field != InputField::Path { return; }
    let input = &app.input_buf;
    if input.is_empty() { return; }

    let expanded = if input.starts_with('~') {
        dirs::home_dir().map(|h| input.replacen('~', &h.to_string_lossy(), 1)).unwrap_or_else(|| input.clone())
    } else { input.clone() };

    let path = Path::new(&expanded);
    let (dir, prefix) = if path.is_dir() && expanded.ends_with('/') {
        (path.to_path_buf(), String::new())
    } else {
        (path.parent().unwrap_or(Path::new("/")).to_path_buf(),
         path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())
    };

    let Ok(entries) = fs::read_dir(&dir) else { return };
    let mut matches: Vec<String> = entries.filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            if prefix.is_empty() || n.starts_with(&prefix) { Some(e.path().to_string_lossy().to_string() + "/") }
            else { None }
        }).collect();
    matches.sort();

    if matches.len() == 1 {
        let c = &matches[0];
        app.input_buf = if input.starts_with('~') {
            dirs::home_dir().and_then(|h| { let s = h.to_string_lossy().to_string(); c.strip_prefix(&s).map(|r| format!("~{r}")) }).unwrap_or_else(|| c.clone())
        } else { c.clone() };
    } else if matches.len() > 1 {
        let common = common_prefix(&matches);
        if common.len() > expanded.len() {
            app.input_buf = if input.starts_with('~') {
                dirs::home_dir().and_then(|h| { let s = h.to_string_lossy().to_string(); common.strip_prefix(&s).map(|r| format!("~{r}")) }).unwrap_or(common)
            } else { common };
        } else {
            let names: Vec<&str> = matches.iter()
                .filter_map(|m| Path::new(m.trim_end_matches('/')).file_name())
                .map(|f| f.to_str().unwrap_or("")).collect();
            app.set_message(&names.join("  "));
        }
    }
}

pub fn common_prefix(strings: &[String]) -> String {
    if strings.is_empty() { return String::new(); }
    let first = &strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.chars().zip(s.chars()).enumerate() {
            if a != b { len = len.min(i); break; }
        }
    }
    first[..len].to_string()
}

pub fn commit_input(app: &mut App) {
    let value = app.input_buf.clone();
    app.input_buf.clear();

    let expand = |v: &str| -> String {
        if v.starts_with('~') {
            dirs::home_dir().map(|h| v.replacen('~', &h.to_string_lossy(), 1)).unwrap_or_else(|| v.to_string())
        } else { v.to_string() }
    };

    match app.input_mode {
        InputMode::AddFolder => {
            if app.input_stage == 0 {
                if value.is_empty() { app.input_mode = InputMode::Normal; return; }
                app.pending_folder_name = value;
                app.input_field = InputField::Path;
                app.input_buf = app.workspace.clone();
                app.input_stage = 1;
            } else {
                let path = expand(&value);
                let name = app.pending_folder_name.clone();
                let session = app.session.clone();

                if let Some(win_idx) = tmux::create_folder_window(&session, &name, &path) {
                    app.state.add_folder(name.clone(), path);
                    if let Some(f) = app.state.get_folder_mut(&name) {
                        f.window_index = Some(win_idx.clone());
                    }

                    let fi = app.state.folders.iter().position(|f| f.name == name);
                    if let (Some(ref sidebar), Some(fi)) = (&app.sidebar_pane_id, fi) {
                        tmux::switch_to_folder_window(sidebar, &win_idx, &session);
                        app.active_folder_idx = Some(fi);
                        app.sidebar_pane_id = tmux::sidebar_pane_id();
                        sync_current_window(app);
                    }

                    save_state(&app.state, &app.workspace);
                    app.set_message(&format!("Folder: {name}"));
                } else {
                    app.set_message("Failed to create window");
                }
                app.input_mode = InputMode::Normal;
            }
        }
        InputMode::AddAgent => {
            match app.input_stage {
                0 => {
                    if value.is_empty() { app.input_mode = InputMode::Normal; return; }
                    app.pending_agent_name = value;
                    app.input_field = InputField::Command;
                    app.input_buf = "claude".to_string();
                    app.input_stage = 1;
                }
                1 => {
                    app.pending_agent_cmd = if value.is_empty() { "claude".to_string() } else { value };
                    if let Some(fi) = app.selected_folder_index() {
                        app.input_buf = app.state.folders[fi].path.clone();
                    }
                    app.input_field = InputField::Path;
                    app.input_stage = 2;
                }
                _ => {
                    let Some(fi) = app.selected_folder_index() else {
                        app.input_mode = InputMode::Normal; return;
                    };
                    let wdir = if value.is_empty() { app.state.folders[fi].path.clone() } else { expand(&value) };
                    let cmd = app.pending_agent_cmd.clone();
                    let agent_name = app.pending_agent_name.clone();

                    // Find a pane to split from: focused agent, any visible pane, or sidebar
                    let split_target = app.focused_agent_key.as_ref()
                        .and_then(|key| {
                            for folder in &app.state.folders {
                                for agent in &folder.agents {
                                    if !agent.hidden && format!("{}/{}", agent.folder, agent.name) == *key {
                                        return agent.pane_id.clone();
                                    }
                                }
                            }
                            None
                        })
                        .or_else(|| {
                            // Any visible pane in current window
                            tmux::list_current_window_panes().first().map(|p| p.pane_id.clone())
                        })
                        .or_else(|| {
                            // Last resort: sidebar pane
                            app.sidebar_pane_id.clone()
                        });

                    let Some(ref target) = split_target else {
                        app.set_message("No pane available");
                        app.input_mode = InputMode::Normal;
                        return;
                    };

                    // Always create a new split pane for the agent
                    let shell_cmd = if !cmd.is_empty() && cmd != "zsh" && cmd != "bash" && cmd != "fish" && cmd != "sh" {
                        cmd.clone()
                    } else {
                        String::new()
                    };

                    if let Some(new_pane_id) = tmux::split_v(target, &shell_cmd, &wdir) {
                        tmux::set_pane_title(&new_pane_id, &agent_name);
                        tmux::select_pane(&new_pane_id);
                        app.focused_agent_key = Some(format!(
                            "{}/{}",
                            app.state.folders[fi].name, agent_name
                        ));
                        app.set_message(&format!("Launched: {agent_name}"));
                    } else {
                        app.set_message("Failed to create pane");
                    }

                    save_state(&app.state, &app.workspace);
                    app.input_mode = InputMode::Normal;
                }
            }
        }
        _ => { app.input_mode = InputMode::Normal; }
    }
}
