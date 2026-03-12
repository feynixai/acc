use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};
use std::time::Duration;
use std::path::Path;

const IDLE_THRESHOLD: u64 = 30;

fn tmux(args: &[&str]) -> String {
    let mut cmd = Command::new("tmux");
    for a in args { cmd.arg(a); }
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

/// Find the first `acc-*` tmux session.
fn find_acc_session() -> Option<String> {
    let raw = tmux(&["list-sessions", "-F", "#{session_name}"]);
    raw.lines()
        .find(|s| s.starts_with("acc-"))
        .map(|s| s.to_string())
}

struct AgentInfo {
    pane_id: String,
    title: String,
    command: String,
    window_name: String,
    active: bool,
}

/// List all non-sidebar panes across all windows in the session.
fn list_all_agents(session: &str) -> Vec<AgentInfo> {
    let raw = tmux(&[
        "list-panes", "-s", "-t", session,
        "-F", "#{pane_id}|#{pane_title}|#{pane_current_command}|#{window_name}|#{pane_last_activity}",
    ]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    raw.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 { return None; }
            let title = parts[1];
            if title == "acc-sidebar" { return None; }
            let activity: u64 = parts[4].parse().unwrap_or(0);
            Some(AgentInfo {
                pane_id: parts[0].to_string(),
                title: title.to_string(),
                command: parts[2].to_string(),
                window_name: parts[3].to_string(),
                active: now.saturating_sub(activity) <= IDLE_THRESHOLD,
            })
        })
        .collect()
}

/// Resolve an agent query to a matching pane.
/// Matches: exact pane_id (%N), exact title (case-insensitive), partial title, command name.
fn find_agent(query: &str, agents: &[AgentInfo]) -> Option<usize> {
    let q = query.to_lowercase();
    // Exact pane_id
    if query.starts_with('%') {
        if let Some(i) = agents.iter().position(|a| a.pane_id == query) {
            return Some(i);
        }
    }
    // Exact title match (case-insensitive)
    if let Some(i) = agents.iter().position(|a| a.title.to_lowercase() == q) {
        return Some(i);
    }
    // Partial title contains
    if let Some(i) = agents.iter().position(|a| a.title.to_lowercase().contains(&q)) {
        return Some(i);
    }
    // Command name match
    agents.iter().position(|a| a.command.to_lowercase().contains(&q))
}

// ── Subcommands ──

pub fn list_agents() {
    let session = match find_acc_session() {
        Some(s) => s,
        None => { eprintln!("No acc session found."); return; }
    };
    let agents = list_all_agents(&session);
    if agents.is_empty() {
        println!("No agents running.");
        return;
    }

    println!("{:<12} {:<20} {:<10} {}", "FOLDER", "AGENT", "STATUS", "PANE");
    for a in &agents {
        let status = if a.active { "● active" } else { "○ idle" };
        let name = if a.title.is_empty() || a.title == a.pane_id {
            &a.command
        } else {
            &a.title
        };
        println!("{:<12} {:<20} {:<10} {}", a.window_name, name, status, a.pane_id);
    }
}

pub fn read_agent(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: acc read <agent> [lines]");
        return;
    }
    let session = match find_acc_session() {
        Some(s) => s,
        None => { eprintln!("No acc session found."); return; }
    };
    let agents = list_all_agents(&session);
    let idx = match find_agent(&args[0], &agents) {
        Some(i) => i,
        None => { eprintln!("Agent '{}' not found.", args[0]); return; }
    };
    let lines = args.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(50);
    let start = format!("-{}", lines);
    let output = tmux(&["capture-pane", "-t", &agents[idx].pane_id, "-p", "-S", &start]);
    println!("{}", output);
}

pub fn send_to_agent(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: acc send <agent> <prompt>");
        return;
    }
    let session = match find_acc_session() {
        Some(s) => s,
        None => { eprintln!("No acc session found."); return; }
    };
    let agents = list_all_agents(&session);
    let idx = match find_agent(&args[0], &agents) {
        Some(i) => i,
        None => { eprintln!("Agent '{}' not found.", args[0]); return; }
    };
    let text = args[1..].join(" ");
    tmux(&["send-keys", "-t", &agents[idx].pane_id, &text, "Enter"]);
    println!("Sent to {} ({})", agents[idx].title, agents[idx].pane_id);
}

pub fn start_root() {
    let session = match find_acc_session() {
        Some(s) => s,
        None => { eprintln!("No acc session found."); return; }
    };

    // Check if ★ root window already exists
    let windows = tmux(&["list-windows", "-t", &session, "-F", "#{window_name}"]);
    if windows.lines().any(|w| w == "★ root") {
        tmux(&["select-window", "-t", &format!("{session}:★ root")]);
        println!("Switched to existing root commander.");
        return;
    }

    // Determine workspace root from session's first window's pane cwd,
    // or fall back to state.json path
    let workspace_root = find_workspace_root(&session);

    // Create new window
    tmux(&["new-window", "-t", &session, "-n", "★ root", "-c", &workspace_root]);

    // Write ROOT_CLAUDE.md
    let home = env::var("HOME").unwrap_or_default();
    let acc_dir = format!("{home}/.agent-center");
    let _ = fs::create_dir_all(&acc_dir);
    let md_path = format!("{acc_dir}/ROOT_CLAUDE.md");
    fs::write(&md_path, ROOT_CLAUDE_MD).ok();

    // Start claude in the new window
    tmux(&["send-keys", "-t", &format!("{session}:★ root"), "claude", "Enter"]);

    // Give claude a moment to start, then send initial prompt
    thread::sleep(Duration::from_secs(3));
    let prompt = format!("Read {} then run acc list", md_path);
    tmux(&["send-keys", "-t", &format!("{session}:★ root"), &prompt, "Enter"]);

    println!("Root commander started in ★ root window.");
}

fn find_workspace_root(session: &str) -> String {
    // Get the cwd of the first pane in the first window
    let raw = tmux(&[
        "list-panes", "-s", "-t", session,
        "-F", "#{pane_current_path}|#{window_index}",
    ]);
    // Find the first window's first pane cwd
    if let Some(line) = raw.lines().next() {
        if let Some((path, _)) = line.split_once('|') {
            if !path.is_empty() {
                // Go up to the workspace root (parent of any folder)
                return path.to_string();
            }
        }
    }
    // Fallback: look for state.json to derive workspace
    let home = env::var("HOME").unwrap_or_default();
    let ws_dir = format!("{home}/.agent-center/workspaces");
    if let Ok(entries) = fs::read_dir(&ws_dir) {
        for entry in entries.flatten() {
            let state_path = entry.path().join("state.json");
            if state_path.exists() {
                if let Ok(data) = fs::read_to_string(&state_path) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                        if let Some(path) = parsed["folders"][0]["path"].as_str() {
                            // Use the parent of the first folder's path
                            if let Some(parent) = Path::new(path).parent() {
                                return parent.to_string_lossy().to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/tmp".into())
}

const ROOT_CLAUDE_MD: &str = r#"# Root Commander

You are the Root Commander — an orchestrator agent that manages other agents running in this tmux session.

## Available CLI Tools

### `acc list`
Lists all running agents with their folder, name, status, and pane ID.

### `acc read <agent> [lines]`
Reads the last N lines (default 50) of an agent's terminal output.
- `acc read claude` — read output from the agent named "claude"
- `acc read %5 100` — read last 100 lines from pane %5

### `acc send <agent> <prompt>`
Sends text followed by Enter to an agent's pane.
- `acc send claude "explain this function"`
- `acc send %5 "run the tests"`

## Agent Resolution
Agents can be referenced by:
1. **Pane ID** — exact match (e.g., `%5`)
2. **Title** — exact match, case-insensitive (e.g., `Claude Code`)
3. **Partial title** — substring match (e.g., `claude`)
4. **Command** — process name match (e.g., `node`)

## Your Role
1. Start by running `acc list` to see available agents
2. Use `acc read` to check on agent progress
3. Use `acc send` to give agents instructions
4. Coordinate work across agents — break tasks down and delegate
5. Monitor progress and intervene when agents get stuck
"#;
