# ACC — Agent Command Center

A tmux-based TUI sidebar for managing teams of CLI agents. Think VS Code's sidebar, but for terminal agents.

**Folder = tmux window. Agent = tmux pane.**

```
┌──────────┬──────────────────────────┐
│ ACC      │                          │
│          │   claude (agent pane)    │
│ ▾ BB   2 │                          │
│   ● claude│──────────────────────────│
│   ● shell │                          │
│ ▸ Docs  1 │   shell (agent pane)     │
│   ○ idle  │                          │
│          │                          │
│──────────│                          │
│ 📁+ ＋ 🗑 │                          │
│          │                          │
│ 1-9 jump │                          │
│ [/] win  │                          │
└──────────┴──────────────────────────┘
```

## Install

```bash
# Build from source (requires Rust and tmux)
cargo build --release
cp target/release/acc ~/.local/bin/   # or ~/bin/, /usr/local/bin/
```

## Usage

```bash
acc              # Launch (creates tmux session with sidebar)
acc kill         # Close the sidebar
acc help         # Show help
```

If already inside tmux, `acc` toggles the sidebar in the current session.

## Key Bindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `g` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+d` | Scroll down 10 |
| `Ctrl+u` | Scroll up 10 |
| `Enter` | Focus selected (folder = switch window, agent = focus pane) |
| `1`-`9` | Quick-jump to agent by number |

### Windows & Panes

| Key | Action |
|-----|--------|
| `[` / `]` | Previous / next folder (window) |
| `n` / `p` | Next / previous agent (pane) |
| `h` | Split horizontal (top/bottom) |
| `v` | Split vertical (left/right) |

### Agent Management

| Key | Action |
|-----|--------|
| `f` | Add folder (creates new tmux window) |
| `a` | Add agent (creates new pane with command) |
| `d` | Close selected pane |
| `s` | Stop agent (Ctrl-C + /exit) |
| `r` | Restart selected agent |
| `x` | Delete selected (with confirmation) |
| `u` | Undo last delete |

### Command Mode

| Key | Action |
|-----|--------|
| `:` | Enter command mode |
| `/` | Search agents/folders |
| `>` | Send text to focused agent |
| `;` / `,` | Next / previous search match |

### Commands (`:` mode)

| Command | Action |
|---------|--------|
| `:q` / `:quit` | Quit ACC |
| `:w <n\|name>` | Jump to window by number or name |
| `:p <n\|name>` | Jump to pane by number or name |
| `:send <text>` | Send text to focused agent |
| `:stop` | Stop focused agent |
| `:restart` | Restart focused agent |
| `:rename <name>` | Rename selected agent or folder |

### Other

| Key | Action |
|-----|--------|
| `q` | Quit sidebar |
| `Ctrl+c` | Quit sidebar |

## Architecture

```
src/
├── main.rs      # Entry point, CLI args, tmux session bootstrap
├── lib.rs       # Module declarations
├── app.rs       # App state, tree model, input modes
├── event.rs     # Event loop (keys, mouse, tick)
├── handler.rs   # Core logic (sync, navigation, splits, undo)
├── command.rs   # Command mode (:), search (/), send (>)
├── ui.rs        # Rendering with ratatui
├── state.rs     # Persistent state (JSON), serde models
└── tmux.rs      # All tmux interactions, pane sync
```

### How it works

- **State is minimal**: Only folder names and paths are persisted to `~/.agent-center/workspaces/<dir>/state.json`. Agents are runtime-only — rebuilt from live tmux panes every 500ms tick.
- **Sidebar is a tmux pane**: ACC runs as a left-side pane (25% width) marked with the title `acc-sidebar`. When you switch folders, the sidebar physically moves between tmux windows via `join-pane`.
- **No duplication**: Agent lists are rebuilt from scratch each tick using `build_agents_from_panes()` — a pure function that maps tmux pane info to agent state. Names are preserved across ticks via pane ID matching.
- **Undo stack**: Deleting agents or folders pushes an entry onto the undo stack. `u` recreates the pane/window with the same command, working directory, and name.

### Per-workspace state

State is stored per working directory:

```
~/.agent-center/
└── workspaces/
    └── Users_arun_Desktop_BB/
        └── state.json
```

## Requirements

- **tmux** (tested with 3.x)
- **Rust** 1.70+ (to build)
- macOS or Linux

## License

MIT
