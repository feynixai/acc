use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub name: String,
    pub folder: String,
    pub working_dir: String,
    #[serde(default = "default_command")]
    pub command: String,
    #[serde(skip)]
    pub pane_id: Option<String>,
    #[serde(skip)]
    pub status: AgentStatus,
    #[serde(skip)]
    pub git_branch: String,
    /// Runtime: true when pane is hidden in background window
    #[serde(skip)]
    pub hidden: bool,
}

fn default_command() -> String {
    "claude".to_string()
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AgentStatus {
    Active,
    Idle,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderState {
    pub name: String,
    pub path: String,
    /// Runtime only — rebuilt from tmux panes each tick.
    #[serde(skip)]
    pub agents: Vec<AgentState>,
    /// Runtime: tmux window index for this folder
    #[serde(skip)]
    pub window_index: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub folders: Vec<FolderState>,
}

impl AppState {
    pub fn get_folder(&self, name: &str) -> Option<&FolderState> {
        self.folders.iter().find(|f| f.name == name)
    }

    pub fn get_folder_mut(&mut self, name: &str) -> Option<&mut FolderState> {
        self.folders.iter_mut().find(|f| f.name == name)
    }

    pub fn add_folder(&mut self, name: String, path: String) {
        if self.get_folder(&name).is_none() {
            self.folders.push(FolderState {
                name,
                path,
                agents: Vec::new(),
                window_index: None,
            });
        }
    }

    pub fn remove_folder(&mut self, name: &str) {
        self.folders.retain(|f| f.name != name);
    }

    pub fn all_agents(&self) -> Vec<&AgentState> {
        self.folders.iter().flat_map(|f| f.agents.iter()).collect()
    }
}

fn state_path(workspace: &str) -> PathBuf {
    let sanitized = workspace.trim_start_matches('/').replace('/', "_");
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agent-center")
        .join("workspaces")
        .join(if sanitized.is_empty() { "default".to_string() } else { sanitized });
    fs::create_dir_all(&dir).ok();
    dir.join("state.json")
}

pub fn load_state(workspace: &str) -> AppState {
    let path = state_path(workspace);
    if !path.exists() {
        return AppState::default();
    }
    let mut state: AppState = match fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => AppState::default(),
    };
    // Remove duplicate folders that share the same base name (e.g. "BB" and "BB-1")
    let base_names: Vec<String> = state.folders.iter().map(|f| f.name.clone()).collect();
    state.folders.retain(|f| {
        // Keep if name doesn't match pattern "basename-N" where basename exists
        if let Some(dash_pos) = f.name.rfind('-') {
            let base = &f.name[..dash_pos];
            let suffix = &f.name[dash_pos + 1..];
            if suffix.chars().all(|c| c.is_ascii_digit()) && base_names.contains(&base.to_string()) {
                return false; // Drop "BB-1" if "BB" exists
            }
        }
        true
    });
    state
}

pub fn save_state(state: &AppState, workspace: &str) {
    let path = state_path(workspace);
    if let Ok(data) = serde_json::to_string_pretty(state) {
        fs::write(path, data).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agents_not_serialized() {
        let mut state = AppState::default();
        state.add_folder("test".into(), "/tmp/test".into());
        if let Some(f) = state.get_folder_mut("test") {
            f.agents.push(AgentState {
                name: "agent1".into(),
                folder: "test".into(),
                working_dir: "/tmp".into(),
                command: "claude".into(),
                pane_id: Some("%42".into()),
                status: AgentStatus::Active,
                git_branch: "main".into(),
                hidden: false,
            });
        }

        let json = serde_json::to_string(&state).unwrap();
        // agents should NOT appear in JSON (serde skip)
        assert!(!json.contains("agent1"));
        assert!(!json.contains("pane_id"));
        // folders should appear
        assert!(json.contains("test"));
        assert!(json.contains("/tmp/test"));
    }

    #[test]
    fn test_load_old_state_with_agents_ignored() {
        // Simulate old state.json that had agents serialized
        let old_json = r#"{
            "folders": [{
                "name": "BB",
                "path": "/Users/test/BB",
                "agents": [
                    {"name": "old-agent", "folder": "BB", "working_dir": "/tmp", "command": "claude"}
                ]
            }]
        }"#;
        let state: AppState = serde_json::from_str(old_json).unwrap();
        assert_eq!(state.folders.len(), 1);
        assert_eq!(state.folders[0].name, "BB");
        // agents should be empty (serde skip = default = empty vec)
        assert!(state.folders[0].agents.is_empty());
    }

    #[test]
    fn test_dedup_removes_suffixed_duplicates() {
        let json = r#"{
            "folders": [
                {"name": "BB", "path": "/Users/test/BB"},
                {"name": "BB-1", "path": "/Users/test/BB"},
                {"name": "BB-2", "path": "/Users/test/BB"},
                {"name": "Arun", "path": "/Users/test/BB"},
                {"name": "other", "path": "/Users/test/other"}
            ]
        }"#;
        let mut state: AppState = serde_json::from_str(json).unwrap();
        // Apply same dedup as load_state
        let base_names: Vec<String> = state.folders.iter().map(|f| f.name.clone()).collect();
        state.folders.retain(|f| {
            if let Some(dash_pos) = f.name.rfind('-') {
                let base = &f.name[..dash_pos];
                let suffix = &f.name[dash_pos + 1..];
                if suffix.chars().all(|c| c.is_ascii_digit()) && base_names.contains(&base.to_string()) {
                    return false;
                }
            }
            true
        });
        // BB-1 and BB-2 removed (suffixed duplicates of BB), Arun and other kept
        assert_eq!(state.folders.len(), 3);
        assert_eq!(state.folders[0].name, "BB");
        assert_eq!(state.folders[1].name, "Arun");
        assert_eq!(state.folders[2].name, "other");
    }

    #[test]
    fn test_add_remove_folder() {
        let mut state = AppState::default();
        state.add_folder("proj".into(), "/tmp/proj".into());
        assert_eq!(state.folders.len(), 1);

        // No duplicate
        state.add_folder("proj".into(), "/tmp/proj".into());
        assert_eq!(state.folders.len(), 1);

        state.remove_folder("proj");
        assert!(state.folders.is_empty());
    }
}
