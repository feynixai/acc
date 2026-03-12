use crate::state::AppState;
use ratatui::widgets::ListState;

pub struct App {
    pub state: AppState,
    pub list_state: ListState,
    pub sidebar_pane_id: Option<String>,
    pub active_folder_idx: Option<usize>,
    pub focused_agent_key: Option<String>,
    pub session: String,
    pub workspace: String,
    pub input_mode: InputMode,
    pub input_buf: String,
    pub input_field: InputField,
    pub input_stage: u8,
    pub pending_folder_name: String,
    pub pending_agent_name: String,
    pub pending_agent_cmd: String,
    pub message: Option<String>,
    pub message_timer: u8,
    pub last_click_time: std::time::Instant,
    pub last_click_row: Option<u16>,
    pub confirm_delete: Option<TreeEntry>,
    pub cmd_buf: String,
    pub search_buf: String,
    pub search_matches: Vec<usize>,
    pub search_idx: usize,
    pub quit_requested: bool,
    pub undo_stack: Vec<UndoEntry>,
}

/// Captures enough info to recreate a deleted agent or folder.
#[derive(Debug, Clone)]
pub enum UndoEntry {
    Agent {
        folder_name: String,
        agent_name: String,
        command: String,
        working_dir: String,
    },
    Folder {
        name: String,
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode { Normal, AddFolder, AddAgent, Command, SendPrompt, Search }

#[derive(Debug, Clone, PartialEq)]
pub enum InputField { Name, Path, Command }

impl App {
    pub fn new(state: AppState, session: String, workspace: String) -> Self {
        let mut ls = ListState::default();
        if !state.folders.is_empty() { ls.select(Some(0)); }
        Self {
            state, list_state: ls, sidebar_pane_id: None,
            active_folder_idx: None, focused_agent_key: None,
            session, workspace,
            input_mode: InputMode::Normal, input_buf: String::new(),
            input_field: InputField::Name, input_stage: 0,
            pending_folder_name: String::new(), pending_agent_name: String::new(),
            pending_agent_cmd: String::new(), message: None, message_timer: 0,
            last_click_time: std::time::Instant::now(),
            last_click_row: None,
            confirm_delete: None,
            cmd_buf: String::new(),
            search_buf: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            quit_requested: false,
            undo_stack: Vec::new(),
        }
    }

    pub fn tree_items(&self) -> Vec<TreeEntry> {
        let mut items = Vec::new();
        for (fi, folder) in self.state.folders.iter().enumerate() {
            items.push(TreeEntry::Folder(fi));
            for (ai, _) in folder.agents.iter().enumerate() {
                items.push(TreeEntry::Agent(fi, ai));
            }
        }
        items
    }

    pub fn selected_entry(&self) -> Option<TreeEntry> {
        let items = self.tree_items();
        self.list_state.selected().and_then(|i| items.get(i).cloned())
    }

    pub fn selected_folder_index(&self) -> Option<usize> {
        match self.selected_entry()? {
            TreeEntry::Folder(fi) | TreeEntry::Agent(fi, _) => Some(fi),
        }
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = Some(msg.to_string());
        self.message_timer = 10;
    }

    pub fn tick_message(&mut self) {
        if self.message_timer > 0 {
            self.message_timer -= 1;
            if self.message_timer == 0 { self.message = None; }
        }
    }
}

#[derive(Debug, Clone)]
pub enum TreeEntry { Folder(usize), Agent(usize, usize) }

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::state::{AgentState, AgentStatus, FolderState};

    pub fn make_app(folders: Vec<FolderState>) -> App {
        let state = AppState { folders };
        App::new(state, "test-session".into(), "/tmp/workspace".into())
    }

    pub fn make_agent(name: &str, folder: &str, status: AgentStatus, pane_id: Option<&str>) -> AgentState {
        AgentState {
            name: name.into(),
            folder: folder.into(),
            working_dir: "/tmp".into(),
            command: "claude".into(),
            pane_id: pane_id.map(|s| s.into()),
            status,
            git_branch: String::new(),
        }
    }

    pub fn make_folder(name: &str, agents: Vec<AgentState>) -> FolderState {
        FolderState {
            name: name.into(),
            path: "/tmp".into(),
            agents,
            window_index: Some("1".into()),
        }
    }

    // ── Tree building ──

    #[test]
    fn test_tree_items_empty() {
        let app = make_app(vec![]);
        assert!(app.tree_items().is_empty());
    }

    #[test]
    fn test_tree_items_folders_only() {
        let app = make_app(vec![
            make_folder("BB", vec![]),
            make_folder("Arun", vec![]),
        ]);
        let items = app.tree_items();
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], TreeEntry::Folder(0)));
        assert!(matches!(items[1], TreeEntry::Folder(1)));
    }

    #[test]
    fn test_tree_items_with_agents() {
        let app = make_app(vec![
            make_folder("BB", vec![
                make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
                make_agent("claude-11", "BB", AgentStatus::Idle, Some("%11")),
            ]),
            make_folder("Arun", vec![
                make_agent("shell-20", "Arun", AgentStatus::Active, Some("%20")),
            ]),
        ]);
        let items = app.tree_items();
        assert_eq!(items.len(), 5);
        assert!(matches!(items[0], TreeEntry::Folder(0)));
        assert!(matches!(items[1], TreeEntry::Agent(0, 0)));
        assert!(matches!(items[2], TreeEntry::Agent(0, 1)));
        assert!(matches!(items[3], TreeEntry::Folder(1)));
        assert!(matches!(items[4], TreeEntry::Agent(1, 0)));
    }

    // ── Selection ──

    #[test]
    fn test_selected_entry_none_when_empty() {
        let app = make_app(vec![]);
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_selected_entry_folder() {
        let mut app = make_app(vec![
            make_folder("BB", vec![
                make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
            ]),
        ]);
        app.list_state.select(Some(0));
        assert!(matches!(app.selected_entry(), Some(TreeEntry::Folder(0))));
    }

    #[test]
    fn test_selected_entry_agent() {
        let mut app = make_app(vec![
            make_folder("BB", vec![
                make_agent("shell-10", "BB", AgentStatus::Active, Some("%10")),
            ]),
        ]);
        app.list_state.select(Some(1));
        assert!(matches!(app.selected_entry(), Some(TreeEntry::Agent(0, 0))));
    }

    #[test]
    fn test_selected_entry_out_of_bounds() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.list_state.select(Some(99));
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_selected_folder_index_from_folder() {
        let mut app = make_app(vec![
            make_folder("BB", vec![]),
            make_folder("Arun", vec![]),
        ]);
        app.list_state.select(Some(1));
        assert_eq!(app.selected_folder_index(), Some(1));
    }

    #[test]
    fn test_selected_folder_index_from_agent() {
        let mut app = make_app(vec![
            make_folder("BB", vec![
                make_agent("a1", "BB", AgentStatus::Active, Some("%1")),
            ]),
            make_folder("Arun", vec![
                make_agent("a2", "Arun", AgentStatus::Active, Some("%2")),
            ]),
        ]);
        // Tree: Folder(0), Agent(0,0), Folder(1), Agent(1,0)
        app.list_state.select(Some(3));
        assert_eq!(app.selected_folder_index(), Some(1));
    }

    // ── Message lifecycle ──

    #[test]
    fn test_message_set_and_tick() {
        let mut app = make_app(vec![]);
        app.set_message("hello");
        assert_eq!(app.message.as_deref(), Some("hello"));
        assert_eq!(app.message_timer, 10);

        for _ in 0..9 {
            app.tick_message();
            assert!(app.message.is_some());
        }
        app.tick_message(); // 10th tick
        assert!(app.message.is_none());
        assert_eq!(app.message_timer, 0);
    }

    #[test]
    fn test_message_overwrite() {
        let mut app = make_app(vec![]);
        app.set_message("first");
        app.tick_message();
        app.set_message("second");
        assert_eq!(app.message.as_deref(), Some("second"));
        assert_eq!(app.message_timer, 10);
    }

    #[test]
    fn test_tick_message_no_message() {
        let mut app = make_app(vec![]);
        app.tick_message();
        assert!(app.message.is_none());
        assert_eq!(app.message_timer, 0);
    }

    // ── Input mode / initial state ──

    #[test]
    fn test_app_starts_normal_mode() {
        let app = make_app(vec![]);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buf.is_empty());
    }

    #[test]
    fn test_initial_selection_on_nonempty() {
        let app = make_app(vec![make_folder("BB", vec![])]);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_initial_selection_on_empty() {
        let app = make_app(vec![]);
        assert_eq!(app.list_state.selected(), None);
    }

    // ── NEW: command mode fields initialized ──

    #[test]
    fn test_command_mode_fields_initialized() {
        let app = make_app(vec![]);
        assert!(app.cmd_buf.is_empty());
        assert!(app.search_buf.is_empty());
        assert!(app.search_matches.is_empty());
        assert_eq!(app.search_idx, 0);
    }

    // ── NEW: all input mode variants ──

    #[test]
    fn test_all_input_mode_variants() {
        let modes = vec![
            InputMode::Normal,
            InputMode::AddFolder,
            InputMode::AddAgent,
            InputMode::Command,
            InputMode::Search,
            InputMode::SendPrompt,
        ];
        // Verify all variants are distinct
        for (i, a) in modes.iter().enumerate() {
            for (j, b) in modes.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // ── NEW: quit_requested default ──

    #[test]
    fn test_quit_requested_default() {
        let app = make_app(vec![]);
        assert!(!app.quit_requested);
    }

    // ── NEW: tree items many folders ──

    #[test]
    fn test_tree_items_many_folders() {
        let folders: Vec<_> = (0..20).map(|i| make_folder(&format!("F{i}"), vec![])).collect();
        let app = make_app(folders);
        assert_eq!(app.tree_items().len(), 20);
    }

    // ── NEW: tree items many agents ──

    #[test]
    fn test_tree_items_many_agents() {
        let agents: Vec<_> = (0..50)
            .map(|i| make_agent(&format!("a-{i}"), "BB", AgentStatus::Active, Some(&format!("%{i}"))))
            .collect();
        let app = make_app(vec![make_folder("BB", agents)]);
        // 1 folder + 50 agents = 51
        assert_eq!(app.tree_items().len(), 51);
    }

    // ── NEW: selected_folder_index consistency ──

    #[test]
    fn test_selected_folder_index_consistency() {
        let mut app = make_app(vec![
            make_folder("A", vec![
                make_agent("a1", "A", AgentStatus::Active, Some("%1")),
                make_agent("a2", "A", AgentStatus::Active, Some("%2")),
            ]),
            make_folder("B", vec![
                make_agent("b1", "B", AgentStatus::Active, Some("%3")),
            ]),
        ]);
        // Tree: Folder(0), Agent(0,0), Agent(0,1), Folder(1), Agent(1,0)
        let expected = [Some(0), Some(0), Some(0), Some(1), Some(1)];
        for (i, exp) in expected.iter().enumerate() {
            app.list_state.select(Some(i));
            assert_eq!(app.selected_folder_index(), *exp, "item {i}");
        }
    }

    // ── NEW: message timer no underflow ──

    #[test]
    fn test_message_timer_no_underflow() {
        let mut app = make_app(vec![]);
        for _ in 0..100 { app.tick_message(); }
        assert!(app.message.is_none());
        assert_eq!(app.message_timer, 0);
    }

    // ── NEW: rapid message updates ──

    #[test]
    fn test_rapid_message_updates() {
        let mut app = make_app(vec![]);
        for i in 0..100 { app.set_message(&format!("msg-{i}")); }
        assert_eq!(app.message.as_deref(), Some("msg-99"));
        assert_eq!(app.message_timer, 10);
    }

    // ── Confirm delete state ──

    #[test]
    fn test_confirm_delete_none_by_default() {
        let app = make_app(vec![make_folder("BB", vec![])]);
        assert!(app.confirm_delete.is_none());
    }

    #[test]
    fn test_selected_entry_after_removal() {
        let mut app = make_app(vec![make_folder("BB", vec![])]);
        app.list_state.select(Some(0));
        app.state.remove_folder("BB");
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_double_click_fields_initialized() {
        let app = make_app(vec![]);
        assert_eq!(app.last_click_row, None);
        assert!(app.confirm_delete.is_none());
    }
}
