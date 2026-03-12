use crate::state::{AgentState, AgentStatus, FolderState};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

const IDLE_THRESHOLD: u64 = 30;

/// Run a tmux command. Public so handler can use it for join-pane with custom flags.
pub fn tmux_cmd(args: &[&str]) -> String { tmux(args) }

fn tmux(args: &[&str]) -> String {
    let mut cmd = Command::new("tmux");
    for a in args { cmd.arg(a); }
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn shell(cmd: &str) -> String {
    match Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

// ── Environment ──

pub fn inside_tmux() -> bool { env::var("TMUX").is_ok() }

pub fn is_installed() -> bool {
    Command::new("tmux").arg("-V").output().is_ok()
}

pub fn current_session() -> String {
    tmux(&["display-message", "-p", "#{session_name}"])
}

pub fn current_window() -> String {
    tmux(&["display-message", "-p", "#{window_index}"])
}

pub fn current_pane() -> String {
    tmux(&["display-message", "-p", "#{pane_id}"])
}

pub fn session_name_for(workspace: &str) -> String {
    let name = Path::new(workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());
    format!("acc-{}", name.replace('.', "_"))
}

pub fn session_exists(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Tmux config ──

/// Embedded Claude status script for the tmux status bar.
const CLAUDE_STATUS_SCRIPT: &str = r#"#!/usr/bin/env bash
# Shows Claude Code status in tmux status bar
if pgrep -f "node.*claude" > /dev/null 2>&1; then
  echo "● Claude"
else
  echo "○ Claude"
fi
"#;

/// Ensure the claude status script exists at ~/.config/acc/tmux-claude-status.sh.
fn ensure_claude_status_script() -> String {
    let home = env::var("HOME").unwrap_or_default();
    let dir = format!("{home}/.config/acc");
    let path = format!("{dir}/tmux-claude-status.sh");
    if !Path::new(&path).exists() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(&path, CLAUDE_STATUS_SCRIPT);
        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
        }
    }
    path
}

/// Apply navigation-only keybindings that are safe for any tmux session.
/// Does NOT touch prefix, status bar, pane borders, etc.
pub fn apply_acc_keybindings(session: &str) {
    // Toggle sidebar: Alt-s from any pane.
    // Use run-shell -b so tmux doesn't wait for exit or display signal noise
    // when the sidebar pane (which spawned the toggle) gets killed.
    let toggle_cmd = format!("{} sidebar-toggle", exe_path_for_toggle());
    tmux(&["bind-key", "-n", "M-s", "run-shell", "-b", &toggle_cmd]);

    // Quick navigation (no prefix)
    tmux(&["bind-key", "-n", "M-z", "next-window"]);
    tmux(&["bind-key", "-n", "M-Z", "previous-window"]);
    tmux(&["bind-key", "-n", "M-a", "select-pane", "-t", ":.+"]);
    tmux(&["bind-key", "-n", "M-A", "select-pane", "-t", ":.-"]);

    // Display pane numbers
    tmux(&["bind-key", "-n", "M-q", "display-panes"]);
    tmux(&["set-option", "-t", session, "display-panes-time", "3000"]);
    tmux(&["set-option", "-t", session, "display-panes-colour", "colour245"]);
    tmux(&["set-option", "-t", session, "display-panes-active-colour", "white"]);

    // Jump to pane by number
    for i in 1..=9 {
        let key = format!("M-{i}");
        let target = format!(":.{i}");
        tmux(&["bind-key", "-n", &key, "select-pane", "-t", &target]);
    }
}

/// Apply acc's full tmux config to the given session.
/// If ~/.config/acc/tmux.conf exists, source that instead of built-in defaults.
pub fn apply_acc_tmux_config(session: &str) {
    let home = env::var("HOME").unwrap_or_default();
    let override_conf = format!("{home}/.config/acc/tmux.conf");

    if Path::new(&override_conf).exists() {
        tmux(&["source-file", &override_conf]);
        return;
    }

    let status_script = ensure_claude_status_script();

    // ── Prefix ──
    tmux(&["unbind-key", "-T", "prefix", "C-b"]);
    tmux(&["set-option", "-t", session, "prefix", "M-Space"]);
    tmux(&["bind-key", "M-Space", "send-prefix"]);

    // ── Mouse ──
    tmux(&["set-option", "-t", session, "mouse", "on"]);

    // ── Clipboard ──
    tmux(&["set-option", "-t", session, "set-clipboard", "on"]);
    tmux(&["bind-key", "-T", "copy-mode", "MouseDragEnd1Pane",
        "send-keys", "-X", "copy-pipe-and-cancel", "pbcopy"]);
    tmux(&["bind-key", "-T", "copy-mode-vi", "MouseDragEnd1Pane",
        "send-keys", "-X", "copy-pipe-and-cancel", "pbcopy"]);

    // ── Title / rename protection ──
    tmux(&["set-option", "-t", session, "allow-set-title", "off"]);
    tmux(&["set-option", "-t", session, "allow-rename", "off"]);
    tmux(&["set-window-option", "-t", session, "automatic-rename", "off"]);

    // ── Rename pane: prefix + T ──
    tmux(&["bind-key", "T", "command-prompt", "-p", "Pane title:", "select-pane -T '%%'"]);

    // ── Numbering starts at 1 ──
    tmux(&["set-option", "-t", session, "base-index", "1"]);
    tmux(&["set-window-option", "-t", session, "pane-base-index", "1"]);

    // ── Theme: grey and white, black status bar ──
    tmux(&["set-option", "-t", session, "status-style", "bg=black,fg=white"]);
    tmux(&["set-option", "-t", session, "pane-border-style", "fg=colour250"]);
    tmux(&["set-option", "-t", session, "pane-active-border-style", "fg=white"]);
    tmux(&["set-option", "-t", session, "pane-border-status", "top"]);
    tmux(&["set-option", "-t", session, "pane-border-format", " [#{pane_index}] #T "]);
    tmux(&["set-option", "-t", session, "message-style", "bg=colour250,fg=black"]);
    tmux(&["set-option", "-t", session, "message-command-style", "bg=colour250,fg=black"]);
    tmux(&["set-option", "-t", session, "mode-style", "bg=white,fg=black"]);

    // ── Status bar ──
    tmux(&["set-option", "-t", session, "status-position", "bottom"]);
    tmux(&["set-option", "-t", session, "status-justify", "left"]);
    tmux(&["set-option", "-t", session, "status-left-length", "50"]);
    tmux(&["set-option", "-t", session, "status-right-length", "100"]);
    tmux(&["set-option", "-t", session, "status-interval", "2"]);

    // Refresh status on pane focus
    tmux(&["set-hook", "-t", session, "pane-focus-in", "refresh-client -S"]);

    let status_left = "#[bg=colour255,fg=black,bold]  #S  #[bg=black] #[fg=colour245]P:#{pane_index}/#{window_panes} ";
    tmux(&["set-option", "-t", session, "status-left", status_left]);

    let status_right = format!(
        "#(bash {})  #[fg=white]#{{b:pane_current_path}}  #[fg=colour245]#(cd #{{pane_current_path}} && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '-')  #[fg=white]%a %d %b  %H:%M ",
        status_script
    );
    tmux(&["set-option", "-t", session, "status-right", &status_right]);

    tmux(&["set-window-option", "-t", session, "window-status-format",
        "#[fg=colour245]  #I #W  "]);
    tmux(&["set-window-option", "-t", session, "window-status-current-format",
        "#[bg=colour237,fg=white,bold]  #I #W  "]);
    tmux(&["set-window-option", "-t", session, "window-status-separator", ""]);

    // Navigation keybindings are applied unconditionally via apply_acc_keybindings()
}

/// Get the acc executable path for tmux keybindings.
fn exe_path_for_toggle() -> String {
    env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "acc".to_string())
}

// ── Session creation ──

/// Create a new acc session with sidebar + shell, source user config, attach.
pub fn create_and_attach(session: &str, workspace: &str, exe: &str) {
    let win_name = Path::new(workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "shell".to_string());

    tmux(&["new-session", "-d", "-s", session, "-c", workspace, "-n", &win_name]);

    // Source user's tmux config
    let home = env::var("HOME").unwrap_or_default();
    let conf = format!("{home}/.tmux.conf");
    if Path::new(&conf).exists() {
        tmux(&["source-file", &conf]);
    }

    // Apply acc's full tmux config (after user config so acc layers on top)
    apply_acc_tmux_config(session);

    // Split: sidebar on left (20%)
    let sidebar_cmd = format!("{} sidebar '{}'", exe, workspace);
    tmux(&[
        "split-window", "-hb", "-l", "20%",
        "-t", &format!("{session}:1"),
        &sidebar_cmd,
    ]);

    // Mark sidebar and force 20% width
    let panes = tmux(&["list-panes", "-t", &format!("{session}:1"), "-F", "#{pane_id}"]);
    if let Some(first_pane) = panes.lines().next() {
        tmux(&["resize-pane", "-t", first_pane, "-x", "20%"]);
        tmux(&["select-pane", "-t", first_pane, "-T", "acc-sidebar"]);
    }

    // Focus the right pane (shell)
    if let Some(second_pane) = panes.lines().nth(1) {
        tmux(&["select-pane", "-t", second_pane]);
    }

    // Attach
    let _ = Command::new("tmux")
        .args(["attach-session", "-t", session])
        .status();
}

pub fn attach(session: &str) {
    let _ = Command::new("tmux")
        .args(["attach-session", "-t", session])
        .status();
}

// ── Sidebar ──

pub fn sidebar_pane_id() -> Option<String> {
    // Search ALL panes in the session (-s flag), not just current window.
    // This ensures we find the sidebar even after window switches.
    let raw = tmux(&["list-panes", "-s", "-F", "#{pane_id}|#{pane_title}"]);
    for line in raw.lines() {
        if let Some((id, title)) = line.split_once('|') {
            if title == "acc-sidebar" {
                return Some(id.to_string());
            }
        }
    }
    None
}

pub fn create_sidebar(sidebar_cmd: &str) -> Option<(String, String)> {
    let right_pane = current_pane();
    if right_pane.is_empty() { return None; }

    let sidebar = tmux(&[
        "split-window", "-hb", "-l", "20%",
        "-P", "-F", "#{pane_id}",
        sidebar_cmd,
    ]);
    if sidebar.is_empty() { return None; }

    // Force 20% width — split-window -l can be ignored in some layouts
    tmux(&["resize-pane", "-t", &sidebar, "-x", "20%"]);
    tmux(&["select-pane", "-t", &sidebar, "-T", "acc-sidebar"]);
    tmux(&["select-pane", "-t", &sidebar]);

    Some((sidebar, right_pane))
}

/// Re-enforce sidebar at 20% width. Call after any pane join/break/split.
pub fn resize_sidebar(sidebar_pane: &str) {
    tmux(&["resize-pane", "-t", sidebar_pane, "-x", "20%"]);
}

pub fn kill_sidebar() {
    if let Some(pane_id) = sidebar_pane_id() {
        tmux(&["kill-pane", "-t", &pane_id]);
    }
}

// ── Pane info ──

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub title: String,
    pub command: String,
    pub cwd: String,
}

/// List all non-sidebar panes in a specific window.
pub fn list_window_panes(session: &str, window: &str) -> Vec<PaneInfo> {
    let raw = tmux(&[
        "list-panes", "-t", &format!("{session}:{window}"),
        "-F", "#{pane_id}|#{pane_title}|#{pane_current_command}|#{pane_current_path}",
    ]);
    let mut panes = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 { continue; }
        if parts[1] == "acc-sidebar" { continue; }
        panes.push(PaneInfo {
            pane_id: parts[0].to_string(),
            title: parts[1].to_string(),
            command: parts[2].to_string(),
            cwd: parts[3].to_string(),
        });
    }
    panes
}

/// List all non-sidebar panes in the CURRENT window.
pub fn list_current_window_panes() -> Vec<PaneInfo> {
    let session = current_session();
    let window = current_window();
    list_window_panes(&session, &window)
}

// ── Pane naming ──

/// Check if a pane title is a default/useless value (hostname, shell name, etc.)
pub fn is_default_title(title: &str) -> bool {
    if title.is_empty() { return true; }
    let lower = title.to_lowercase();
    if matches!(lower.as_str(), "bash" | "zsh" | "fish" | "sh") { return true; }
    if title.contains('.') { return true; }
    false
}

/// Derive a display name for a pane.
pub fn pane_display_name(pane: &PaneInfo, folder_name: &str) -> String {
    if !is_default_title(&pane.title) && pane.title != folder_name {
        return pane.title.clone();
    }
    let short_id = pane.pane_id.trim_start_matches('%');
    let cmd = &pane.command;
    if matches!(cmd.as_str(), "zsh" | "bash" | "fish" | "sh") {
        format!("shell-{short_id}")
    } else {
        format!("{cmd}-{short_id}")
    }
}

/// Set a custom title on a tmux pane.
pub fn set_pane_title(pane_id: &str, title: &str) {
    tmux(&["select-pane", "-t", pane_id, "-T", title]);
}

// ── Folder = Window operations ──

/// Create a new tmux window for a folder. Returns window index.
pub fn create_folder_window(session: &str, name: &str, cwd: &str) -> Option<String> {
    let result = tmux(&[
        "new-window", "-t", session, "-n", name, "-c", cwd,
        "-d", "-P", "-F", "#{window_index}",
    ]);
    if result.is_empty() { None } else { Some(result) }
}

/// Switch to a folder's window and move the sidebar there.
///
/// Uses pane IDs (stable %N) rather than window indices (which shift when
/// windows are destroyed). After join-pane the source window may be destroyed
/// if the sidebar was its last pane, so we select-window by pane ID.
pub fn switch_to_folder_window(sidebar_pane: &str, target_window: &str, session: &str) {
    let current_win = current_window();
    if current_win == target_window { return; }

    // Find any non-sidebar pane in the target window to use as join target.
    // This avoids joining to an ambiguous window reference.
    let target_panes = list_window_panes(session, target_window);
    let join_target = if let Some(p) = target_panes.first() {
        p.pane_id.clone()
    } else {
        // Fallback: use session:window format
        format!("{session}:{target_window}")
    };

    // Atomic move: join-pane implicitly breaks the pane from its source window.
    // If the sidebar was the last pane in its old window, that window is destroyed
    // and remaining window indices may shift — that's why we re-detect below.
    tmux(&[
        "join-pane", "-hb", "-l", "20%",
        "-s", sidebar_pane,
        "-t", &join_target,
    ]);

    // Force sidebar to 20% width. join-pane's -l flag is unreliable when the
    // target window has complex layouts — the sidebar can end up squeezed to
    // a sliver. An explicit resize after the join guarantees it.
    tmux(&["resize-pane", "-t", sidebar_pane, "-x", "20%"]);

    // Select the window that now contains the sidebar (using stable pane ID).
    // This works even if window indices shifted after the old window was destroyed.
    tmux(&["select-window", "-t", sidebar_pane]);

    // Re-mark sidebar title and keep focus on TUI
    tmux(&["select-pane", "-t", sidebar_pane, "-T", "acc-sidebar"]);
    tmux(&["select-pane", "-t", sidebar_pane]);
}

/// Re-detect window indices for all folders after a window switch.
/// Window indices can shift when a window is destroyed (e.g., sidebar
/// was the only pane in its old window).
pub fn refresh_folder_windows(folders: &mut [crate::state::FolderState], session: &str) {
    let raw = tmux(&[
        "list-windows", "-t", session, "-F", "#{window_index}|#{window_name}",
    ]);
    let windows: Vec<(&str, &str)> = raw.lines()
        .filter_map(|l| l.split_once('|'))
        .collect();

    for folder in folders.iter_mut() {
        // Match by window name (folder name)
        if let Some((idx, _)) = windows.iter().find(|(_, name)| *name == folder.name) {
            folder.window_index = Some(idx.to_string());
        }
    }
}

/// Kill a tmux window (folder).
pub fn kill_window(session: &str, window: &str) {
    tmux(&["kill-window", "-t", &format!("{session}:{window}")]);
}

/// Rename a tmux window.
pub fn rename_window(session: &str, window: &str, name: &str) {
    tmux(&["rename-window", "-t", &format!("{session}:{window}"), name]);
}

// ── Agent = Pane operations ──

/// Split a pane horizontally (top/bottom). Returns new pane_id.
pub fn split_h(target_pane: &str, cmd: &str, cwd: &str) -> Option<String> {
    let result = tmux(&[
        "split-window", "-v", "-t", target_pane, "-c", cwd,
        "-P", "-F", "#{pane_id}",
    ]);
    if result.is_empty() { return None; }
    if !cmd.is_empty() {
        thread::sleep(Duration::from_millis(100));
        tmux(&["send-keys", "-t", &result, cmd, "Enter"]);
    }
    Some(result)
}

/// Split a pane vertically (left/right). Returns new pane_id.
pub fn split_v(target_pane: &str, cmd: &str, cwd: &str) -> Option<String> {
    let result = tmux(&[
        "split-window", "-h", "-t", target_pane, "-c", cwd,
        "-P", "-F", "#{pane_id}",
    ]);
    if result.is_empty() { return None; }
    if !cmd.is_empty() {
        thread::sleep(Duration::from_millis(100));
        tmux(&["send-keys", "-t", &result, cmd, "Enter"]);
    }
    Some(result)
}

/// Split horizontally, placing new pane BEFORE target (top/left). Returns new pane_id.
pub fn split_h_before(target_pane: &str, cmd: &str, cwd: &str) -> Option<String> {
    let result = tmux(&[
        "split-window", "-vb", "-t", target_pane, "-c", cwd,
        "-P", "-F", "#{pane_id}",
    ]);
    if result.is_empty() { return None; }
    if !cmd.is_empty() {
        thread::sleep(Duration::from_millis(100));
        tmux(&["send-keys", "-t", &result, cmd, "Enter"]);
    }
    Some(result)
}

/// Split vertically, placing new pane BEFORE target (left). Returns new pane_id.
pub fn split_v_before(target_pane: &str, cmd: &str, cwd: &str) -> Option<String> {
    let result = tmux(&[
        "split-window", "-hb", "-t", target_pane, "-c", cwd,
        "-P", "-F", "#{pane_id}",
    ]);
    if result.is_empty() { return None; }
    if !cmd.is_empty() {
        thread::sleep(Duration::from_millis(100));
        tmux(&["send-keys", "-t", &result, cmd, "Enter"]);
    }
    Some(result)
}

/// Focus a pane.
pub fn select_pane(pane_id: &str) {
    tmux(&["select-pane", "-t", pane_id]);
}

/// Zoom (maximize) a pane within its window.
pub fn zoom_pane(pane_id: &str) {
    tmux(&["resize-pane", "-Z", "-t", pane_id]);
}

/// Break a pane to a hidden background window. Process keeps running.
pub fn break_pane_to_background(pane_id: &str, session: &str, folder_name: &str) {
    let bg_window = format!("_acc_bg_{}", folder_name);
    // Check if bg window already exists
    let windows = tmux(&["list-windows", "-t", session, "-F", "#{window_name}"]);
    let exists = windows.lines().any(|w| w == bg_window);
    if exists {
        // Join pane into existing bg window
        let target = format!("{session}:{bg_window}");
        tmux(&["join-pane", "-d", "-t", &target, "-s", pane_id]);
    } else {
        // Break pane to a new window named bg_window
        tmux(&["break-pane", "-d", "-s", pane_id, "-n", &bg_window]);
    }
}

/// Check if a pane is in a background `_acc_bg_*` window.
pub fn is_pane_in_background(pane_id: &str, session: &str) -> bool {
    let raw = tmux(&[
        "list-panes", "-s", "-t", session,
        "-F", "#{pane_id}|#{window_name}",
    ]);
    for line in raw.lines() {
        if let Some((pid, wname)) = line.split_once('|') {
            if pid == pane_id && wname.starts_with("_acc_bg_") {
                return true;
            }
        }
    }
    false
}

/// Join a hidden background pane back into a target window pane.
/// Then re-enforce sidebar at 20% so it doesn't get squished.
pub fn join_pane_back(pane_id: &str, target_pane: &str) {
    tmux(&["join-pane", "-h", "-t", target_pane, "-s", pane_id]);
    // Re-enforce sidebar width if it exists in this window
    if let Some(sidebar) = sidebar_pane_id() {
        tmux(&["resize-pane", "-t", &sidebar, "-x", "20%"]);
    }
}

/// List panes in background windows for a folder.
pub fn list_background_panes(session: &str, folder_name: &str) -> Vec<PaneInfo> {
    let bg_window = format!("_acc_bg_{}", folder_name);
    let raw = tmux(&[
        "list-panes", "-s", "-t", session,
        "-F", "#{pane_id}|#{pane_title}|#{pane_current_command}|#{pane_current_path}|#{window_name}",
    ]);
    let mut panes = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 { continue; }
        if parts[4] != bg_window { continue; }
        if parts[1] == "acc-sidebar" { continue; }
        panes.push(PaneInfo {
            pane_id: parts[0].to_string(),
            title: parts[1].to_string(),
            command: parts[2].to_string(),
            cwd: parts[3].to_string(),
        });
    }
    panes
}

/// Kill a pane (agent).
pub fn kill_pane(pane_id: &str) {
    tmux(&["kill-pane", "-t", pane_id]);
}

/// Send Ctrl-C then /exit to stop an agent process.
pub fn stop_agent_process(pane_id: &str) {
    tmux(&["send-keys", "-t", pane_id, "C-c", ""]);
    thread::sleep(Duration::from_millis(200));
    tmux(&["send-keys", "-t", pane_id, "/exit", "Enter"]);
}

pub fn send_to_pane(pane_id: &str, text: &str) {
    tmux(&["send-keys", "-t", pane_id, text, "Enter"]);
}

pub fn restart_agent(pane_id: &str, command: &str) {
    stop_agent_process(pane_id);
    std::thread::sleep(std::time::Duration::from_millis(500));
    tmux(&["send-keys", "-t", pane_id, command, "Enter"]);
}

// ── Pure logic: build agents from pane list ──

/// Build agent list from pane data. Pure function — no tmux calls.
/// Preserves names from previous agents via pane_id matching.
pub fn build_agents_from_panes(
    panes: &[PaneInfo],
    prev_agents: &[AgentState],
    folder_name: &str,
) -> Vec<AgentState> {
    let name_map: HashMap<String, String> = prev_agents.iter()
        .filter_map(|a| a.pane_id.as_ref().map(|pid| (pid.clone(), a.name.clone())))
        .collect();

    panes.iter().map(|pane| {
        let name = name_map.get(&pane.pane_id)
            .cloned()
            .unwrap_or_else(|| pane_display_name(pane, folder_name));

        AgentState {
            name,
            folder: folder_name.to_string(),
            working_dir: pane.cwd.clone(),
            command: pane.command.clone(),
            pane_id: Some(pane.pane_id.clone()),
            status: AgentStatus::Active,
            git_branch: String::new(),
            hidden: false,
        }
    }).collect()
}

// ── Pane activity status ──

fn pane_activity_status(pane_id: &str) -> AgentStatus {
    let activity = tmux(&["display-message", "-t", pane_id, "-p", "#{pane_last_activity}"]);
    if let Ok(last) = activity.parse::<u64>() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - last > IDLE_THRESHOLD {
            AgentStatus::Idle
        } else {
            AgentStatus::Active
        }
    } else {
        AgentStatus::Active
    }
}

// ── Reconnect ──

/// On startup, match saved folders to existing tmux windows by name.
/// Folders with no matching window are removed (session was killed).
pub fn reconnect_folders(folders: &mut Vec<FolderState>, session: &str) {
    let raw = tmux(&[
        "list-windows", "-t", session, "-F", "#{window_index}|#{window_name}",
    ]);
    let windows: Vec<(&str, &str)> = raw.lines()
        .filter_map(|l| l.split_once('|'))
        .collect();

    for folder in folders.iter_mut() {
        folder.window_index = None; // reset first
        for (idx, name) in &windows {
            if *name == folder.name {
                folder.window_index = Some(idx.to_string());
                break;
            }
        }
    }

    // Drop folders whose tmux windows no longer exist
    folders.retain(|f| f.window_index.is_some());
}

// ── Sync: rebuild agents from tmux panes ──

/// Rebuild a folder's agent list from actual tmux panes.
/// `fetch_git`: if true, also fetch git branch for each pane (expensive).
pub fn sync_folder_panes(folder: &mut FolderState, session: &str, fetch_git: bool) {
    let Some(ref win_idx) = folder.window_index else { return };

    let panes = list_window_panes(session, win_idx);

    // Build via pure function (preserves names)
    let mut agents = build_agents_from_panes(&panes, &folder.agents, &folder.name);

    // Enrich with status and optional git branch
    for (agent, pane) in agents.iter_mut().zip(panes.iter()) {
        agent.status = pane_activity_status(&pane.pane_id);
        if fetch_git && !pane.cwd.is_empty() {
            agent.git_branch = shell(&format!(
                "cd '{}' && git rev-parse --abbrev-ref HEAD 2>/dev/null",
                pane.cwd
            ));
        }
    }

    // Also scan background window for hidden panes
    let bg_panes = list_background_panes(session, &folder.name);
    let mut hidden_agents = build_agents_from_panes(&bg_panes, &folder.agents, &folder.name);
    for (agent, pane) in hidden_agents.iter_mut().zip(bg_panes.iter()) {
        agent.hidden = true;
        agent.status = pane_activity_status(&pane.pane_id);
    }
    agents.extend(hidden_agents);

    // Preserve original agent order from previous tick.
    // New agents (not in prev) go at the end.
    let prev_order: Vec<String> = folder.agents.iter()
        .filter_map(|a| a.pane_id.clone())
        .collect();
    if !prev_order.is_empty() {
        agents.sort_by_key(|a| {
            a.pane_id.as_ref()
                .and_then(|pid| prev_order.iter().position(|p| p == pid))
                .unwrap_or(usize::MAX)
        });
    }

    folder.agents = agents;
}

/// Select the next non-sidebar pane in the current window.
#[allow(dead_code)]
pub fn select_next_pane() {
    let panes = list_current_window_panes();
    if panes.len() < 2 { return; }
    let current = current_pane();
    let idx = panes.iter().position(|p| p.pane_id == current).unwrap_or(0);
    let next = (idx + 1) % panes.len();
    select_pane(&panes[next].pane_id);
}

/// Select the previous non-sidebar pane in the current window.
#[allow(dead_code)]
pub fn select_prev_pane() {
    let panes = list_current_window_panes();
    if panes.len() < 2 { return; }
    let current = current_pane();
    let idx = panes.iter().position(|p| p.pane_id == current).unwrap_or(0);
    let prev = if idx == 0 { panes.len() - 1 } else { idx - 1 };
    select_pane(&panes[prev].pane_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pure logic tests ──

    #[test]
    fn test_session_name_for() {
        assert_eq!(session_name_for("/Users/arun/Desktop/BB"), "acc-BB");
        assert_eq!(session_name_for("/tmp/my.project"), "acc-my_project");
        assert_eq!(session_name_for("/"), "acc-default");
    }

    #[test]
    fn test_is_default_title() {
        assert!(is_default_title(""));
        assert!(is_default_title("bash"));
        assert!(is_default_title("zsh"));
        assert!(is_default_title("fish"));
        assert!(is_default_title("sh"));
        assert!(is_default_title("Aruns-MacBook-Pro.local"));
        assert!(is_default_title("server.example.com"));
        assert!(!is_default_title("fix-auth"));
        assert!(!is_default_title("claude"));
        assert!(!is_default_title("my-agent"));
        assert!(!is_default_title("pane-42"));
    }

    #[test]
    fn test_pane_display_name_custom_title() {
        let pane = PaneInfo {
            pane_id: "%42".into(), title: "fix-auth".into(),
            command: "claude".into(), cwd: "/tmp".into(),
        };
        assert_eq!(pane_display_name(&pane, "BB"), "fix-auth");
    }

    #[test]
    fn test_pane_display_name_hostname_ignored() {
        let pane = PaneInfo {
            pane_id: "%42".into(), title: "Aruns-MacBook-Pro.local".into(),
            command: "zsh".into(), cwd: "/tmp".into(),
        };
        assert_eq!(pane_display_name(&pane, "BB"), "shell-42");
    }

    #[test]
    fn test_pane_display_name_shell() {
        let pane = PaneInfo {
            pane_id: "%10".into(), title: "bash".into(),
            command: "bash".into(), cwd: "/tmp".into(),
        };
        assert_eq!(pane_display_name(&pane, "test"), "shell-10");
    }

    #[test]
    fn test_pane_display_name_process() {
        let pane = PaneInfo {
            pane_id: "%55".into(), title: "".into(),
            command: "claude".into(), cwd: "/tmp".into(),
        };
        assert_eq!(pane_display_name(&pane, "BB"), "claude-55");
    }

    #[test]
    fn test_pane_display_name_folder_name_ignored() {
        let pane = PaneInfo {
            pane_id: "%5".into(), title: "BB".into(),
            command: "zsh".into(), cwd: "/tmp".into(),
        };
        assert_eq!(pane_display_name(&pane, "BB"), "shell-5");
    }

    // ── build_agents_from_panes tests ──

    #[test]
    fn test_build_agents_empty_panes() {
        let agents = build_agents_from_panes(&[], &[], "BB");
        assert!(agents.is_empty());
    }

    #[test]
    fn test_build_agents_new_panes_auto_named() {
        let panes = vec![
            PaneInfo { pane_id: "%10".into(), title: "zsh".into(), command: "zsh".into(), cwd: "/tmp".into() },
            PaneInfo { pane_id: "%11".into(), title: "".into(), command: "claude".into(), cwd: "/tmp".into() },
        ];
        let agents = build_agents_from_panes(&panes, &[], "BB");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "shell-10");
        assert_eq!(agents[0].pane_id, Some("%10".into()));
        assert_eq!(agents[1].name, "claude-11");
        assert_eq!(agents[1].pane_id, Some("%11".into()));
    }

    #[test]
    fn test_build_agents_preserves_names_across_ticks() {
        let panes = vec![
            PaneInfo { pane_id: "%10".into(), title: "zsh".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];
        // First tick: auto-named
        let agents1 = build_agents_from_panes(&panes, &[], "BB");
        assert_eq!(agents1[0].name, "shell-10");

        // User renames agent (simulated by modifying the agent)
        let mut prev = agents1;
        prev[0].name = "my-shell".to_string();

        // Second tick: name preserved via name_map
        let agents2 = build_agents_from_panes(&panes, &prev, "BB");
        assert_eq!(agents2[0].name, "my-shell");
    }

    #[test]
    fn test_build_agents_pane_removed() {
        let panes_before = vec![
            PaneInfo { pane_id: "%10".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
            PaneInfo { pane_id: "%11".into(), title: "".into(), command: "claude".into(), cwd: "/tmp".into() },
        ];
        let prev = build_agents_from_panes(&panes_before, &[], "BB");
        assert_eq!(prev.len(), 2);

        // Pane %11 killed
        let panes_after = vec![
            PaneInfo { pane_id: "%10".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];
        let agents = build_agents_from_panes(&panes_after, &prev, "BB");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pane_id, Some("%10".into()));
    }

    #[test]
    fn test_build_agents_pane_added() {
        let panes_before = vec![
            PaneInfo { pane_id: "%10".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];
        let prev = build_agents_from_panes(&panes_before, &[], "BB");

        // New pane added
        let panes_after = vec![
            PaneInfo { pane_id: "%10".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
            PaneInfo { pane_id: "%12".into(), title: "my-agent".into(), command: "claude".into(), cwd: "/tmp".into() },
        ];
        let agents = build_agents_from_panes(&panes_after, &prev, "BB");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "shell-10"); // preserved
        assert_eq!(agents[1].name, "my-agent"); // custom title
    }

    #[test]
    fn test_build_agents_no_duplication_ever() {
        let panes = vec![
            PaneInfo { pane_id: "%10".into(), title: "zsh".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];

        // Simulate 100 ticks — should never grow beyond 1 agent
        let mut agents = Vec::new();
        for _ in 0..100 {
            agents = build_agents_from_panes(&panes, &agents, "BB");
        }
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "shell-10");
    }

    #[test]
    fn test_build_agents_rapid_pane_changes() {
        // Simulate panes changing rapidly
        let panes1 = vec![
            PaneInfo { pane_id: "%10".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];
        let agents1 = build_agents_from_panes(&panes1, &[], "BB");

        // Pane killed and new one created in same tick
        let panes2 = vec![
            PaneInfo { pane_id: "%20".into(), title: "".into(), command: "zsh".into(), cwd: "/tmp".into() },
        ];
        let agents2 = build_agents_from_panes(&panes2, &agents1, "BB");
        assert_eq!(agents2.len(), 1);
        assert_eq!(agents2[0].pane_id, Some("%20".into()));
        assert_eq!(agents2[0].name, "shell-20"); // new name, old one gone
    }

    #[test]
    fn test_build_agents_hostname_title_gets_proper_name() {
        let panes = vec![
            PaneInfo { pane_id: "%5".into(), title: "server.local".into(), command: "node".into(), cwd: "/app".into() },
        ];
        let agents = build_agents_from_panes(&panes, &[], "myproject");
        assert_eq!(agents[0].name, "node-5"); // hostname ignored, uses command
    }

    #[test]
    fn test_build_agents_custom_title_set_by_user() {
        let panes = vec![
            PaneInfo { pane_id: "%42".into(), title: "fix-auth".into(), command: "claude".into(), cwd: "/app".into() },
        ];
        let agents = build_agents_from_panes(&panes, &[], "BB");
        assert_eq!(agents[0].name, "fix-auth");

        // Name persists across ticks
        let agents2 = build_agents_from_panes(&panes, &agents, "BB");
        assert_eq!(agents2[0].name, "fix-auth");
    }

    #[test]
    fn test_build_agents_all_fields_set() {
        let panes = vec![
            PaneInfo { pane_id: "%99".into(), title: "my-agent".into(), command: "claude".into(), cwd: "/home/user/project".into() },
        ];
        let agents = build_agents_from_panes(&panes, &[], "testfolder");
        let a = &agents[0];
        assert_eq!(a.name, "my-agent");
        assert_eq!(a.folder, "testfolder");
        assert_eq!(a.working_dir, "/home/user/project");
        assert_eq!(a.command, "claude");
        assert_eq!(a.pane_id, Some("%99".into()));
        assert_eq!(a.status, AgentStatus::Active);
        assert_eq!(a.git_branch, "");
    }

    // ── Integration tests (require tmux) ──

    fn tmux_available() -> bool {
        Command::new("tmux").arg("-V").output().map(|o| o.status.success()).unwrap_or(false)
    }

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn create_test_session() -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let session = format!("acc-test-{}-{}", std::process::id(), id);
        tmux(&["new-session", "-d", "-s", &session, "-x", "200", "-y", "50"]);
        thread::sleep(Duration::from_millis(200));
        session
    }

    fn kill_test_session(session: &str) {
        tmux(&["kill-session", "-t", session]);
    }

    #[test]
    fn test_tmux_window_lifecycle() {
        if !tmux_available() { return; }
        let session = create_test_session();

        // Create window
        let win_idx = create_folder_window(&session, "test-folder", "/tmp");
        assert!(win_idx.is_some(), "create_folder_window should return index");
        let win = win_idx.unwrap();

        // Verify window exists by listing windows
        let raw = tmux(&["list-windows", "-t", &session, "-F", "#{window_index}|#{window_name}"]);
        assert!(raw.contains(&format!("{}|test-folder", win)));

        // Rename window
        rename_window(&session, &win, "renamed-folder");
        let raw2 = tmux(&["list-windows", "-t", &session, "-F", "#{window_index}|#{window_name}"]);
        assert!(raw2.contains(&format!("{}|renamed-folder", win)));

        // Kill window
        kill_window(&session, &win);
        let raw3 = tmux(&["list-windows", "-t", &session, "-F", "#{window_index}|#{window_name}"]);
        assert!(!raw3.contains("renamed-folder"));

        kill_test_session(&session);
    }

    #[test]
    fn test_tmux_pane_lifecycle() {
        if !tmux_available() { return; }
        let session = create_test_session();

        // Get default pane
        let panes = list_window_panes(&session, "1");
        assert!(!panes.is_empty(), "session should have at least one pane");
        let first_pane = &panes[0].pane_id;

        // Split horizontal
        let new_h = split_h(first_pane, "", "/tmp");
        assert!(new_h.is_some(), "split_h should return pane id");

        // Split vertical
        let new_v = split_v(first_pane, "", "/tmp");
        assert!(new_v.is_some(), "split_v should return pane id");

        // Verify panes exist
        let panes_after = list_window_panes(&session, "1");
        assert_eq!(panes_after.len(), 3, "should have 3 panes after 2 splits");

        // Set pane title
        let pane_id = new_h.unwrap();
        set_pane_title(&pane_id, "my-custom-name");
        thread::sleep(Duration::from_millis(50));
        let panes_titled = list_window_panes(&session, "1");
        let titled = panes_titled.iter().find(|p| p.pane_id == pane_id);
        assert!(titled.is_some());
        assert_eq!(titled.unwrap().title, "my-custom-name");

        // Kill pane
        kill_pane(&pane_id);
        thread::sleep(Duration::from_millis(50));
        let panes_final = list_window_panes(&session, "1");
        assert_eq!(panes_final.len(), 2, "should have 2 panes after killing one");

        kill_test_session(&session);
    }

    #[test]
    fn test_tmux_sync_integration() {
        if !tmux_available() { return; }
        let session = create_test_session();

        let mut folder = FolderState {
            name: "test".into(),
            path: "/tmp".into(),
            agents: Vec::new(),
            window_index: Some("1".into()),
        };

        // Initial sync — should find the default pane
        sync_folder_panes(&mut folder, &session, false);
        assert_eq!(folder.agents.len(), 1, "should detect the default pane");
        let first_agent_name = folder.agents[0].name.clone();

        // Split a pane
        let first_pane = &folder.agents[0].pane_id.clone().unwrap();
        let new_pane = split_h(first_pane, "", "/tmp").unwrap();
        set_pane_title(&new_pane, "my-agent");
        thread::sleep(Duration::from_millis(50));

        // Sync again — should detect new pane
        sync_folder_panes(&mut folder, &session, false);
        assert_eq!(folder.agents.len(), 2, "should detect both panes");

        // First agent name preserved
        assert_eq!(folder.agents[0].name, first_agent_name);

        // New agent named from title
        let new_agent = folder.agents.iter().find(|a| a.pane_id.as_deref() == Some(&new_pane));
        assert!(new_agent.is_some());
        assert_eq!(new_agent.unwrap().name, "my-agent");

        // Kill the new pane
        kill_pane(&new_pane);
        thread::sleep(Duration::from_millis(50));

        // Sync — should only have 1 agent, no ghosts
        sync_folder_panes(&mut folder, &session, false);
        assert_eq!(folder.agents.len(), 1, "killed pane should be gone");

        // Run sync 50 more times — verify no duplication
        for _ in 0..50 {
            sync_folder_panes(&mut folder, &session, false);
        }
        assert_eq!(folder.agents.len(), 1, "50 syncs should not create duplicates");

        kill_test_session(&session);
    }

    #[test]
    fn test_tmux_sync_sidebar_excluded() {
        if !tmux_available() { return; }
        let session = create_test_session();

        // Simulate sidebar by setting pane title
        let panes = list_window_panes(&session, "1");
        let pane_id = &panes[0].pane_id;
        set_pane_title(pane_id, "acc-sidebar");
        thread::sleep(Duration::from_millis(50));

        // Split to have at least one non-sidebar pane
        split_h(pane_id, "", "/tmp");
        thread::sleep(Duration::from_millis(50));

        let mut folder = FolderState {
            name: "test".into(), path: "/tmp".into(),
            agents: Vec::new(), window_index: Some("1".into()),
        };
        sync_folder_panes(&mut folder, &session, false);

        // Sidebar pane should not appear in agents
        for agent in &folder.agents {
            assert_ne!(agent.pane_id.as_deref(), Some(pane_id.as_str()),
                "sidebar pane should be excluded from agents");
        }

        kill_test_session(&session);
    }

    #[test]
    fn test_tmux_reconnect_folders() {
        if !tmux_available() { return; }
        let session = create_test_session();

        // Create a named window
        create_folder_window(&session, "my-project", "/tmp");
        thread::sleep(Duration::from_millis(50));

        let mut folders = vec![
            FolderState {
                name: "my-project".into(), path: "/tmp".into(),
                agents: Vec::new(), window_index: None,
            },
            FolderState {
                name: "nonexistent".into(), path: "/nope".into(),
                agents: Vec::new(), window_index: None,
            },
        ];

        reconnect_folders(&mut folders, &session);

        // my-project should get a window_index, nonexistent should be removed
        assert_eq!(folders.len(), 1, "unmatched folder should be removed");
        assert_eq!(folders[0].name, "my-project");
        assert!(folders[0].window_index.is_some(),
            "folder matching window name should get reconnected");

        kill_test_session(&session);
    }

    #[test]
    fn test_tmux_multi_window_sync() {
        if !tmux_available() { return; }
        let session = create_test_session();

        // Create second window
        let win2 = create_folder_window(&session, "second", "/tmp").unwrap();
        thread::sleep(Duration::from_millis(50));

        let mut folder1 = FolderState {
            name: "first".into(), path: "/tmp".into(),
            agents: Vec::new(), window_index: Some("1".into()),
        };
        let mut folder2 = FolderState {
            name: "second".into(), path: "/tmp".into(),
            agents: Vec::new(), window_index: Some(win2.clone()),
        };

        // Add a pane to window 2
        let panes2 = list_window_panes(&session, &win2);
        if let Some(p) = panes2.first() {
            split_h(&p.pane_id, "", "/tmp");
        }
        thread::sleep(Duration::from_millis(50));

        // Sync both folders
        sync_folder_panes(&mut folder1, &session, false);
        sync_folder_panes(&mut folder2, &session, false);

        // Each folder should have its own agents
        assert_eq!(folder1.agents.len(), 1, "window 1 has 1 pane");
        assert_eq!(folder2.agents.len(), 2, "window 2 has 2 panes after split");

        // Verify no cross-contamination
        let f1_panes: Vec<_> = folder1.agents.iter().map(|a| a.pane_id.clone()).collect();
        let f2_panes: Vec<_> = folder2.agents.iter().map(|a| a.pane_id.clone()).collect();
        for p in &f1_panes {
            assert!(!f2_panes.contains(p), "folders should not share pane_ids");
        }

        kill_test_session(&session);
    }
}
