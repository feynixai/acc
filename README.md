<p align="center">
  <img src="assets/logo.png" alt="FeynixAI" width="200" />
</p>

<h1 align="center">ACC — Agent Command Center</h1>

<p align="center">
  <strong>by <a href="https://github.com/feynixai">FeynixAI</a></strong><br/>
  A tmux-based TUI for managing teams of CLI agents.
</p>

<p align="center">
  <a href="https://github.com/feynixai/acc/releases"><img src="https://img.shields.io/github/v/release/feynixai/acc?style=flat-square" alt="Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT License" /></a>
</p>

---

Think VS Code's sidebar, but for terminal agents.

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
└──────────┴──────────────────────────┘
```

## Requirements

- **tmux** 3.x+ (`brew install tmux` on macOS)
- **Rust** 1.70+ (to build from source)
- macOS or Linux

## Install

### From source (recommended)

```bash
git clone https://github.com/feynixai/acc.git
cd acc
cargo build --release
```

Then copy the binary to your PATH:

```bash
# Pick one:
cp target/release/acc /usr/local/bin/
# or
cp target/release/acc ~/.local/bin/
# or
cp target/release/acc ~/bin/
```

Verify it works:

```bash
acc --help
```

### One-liner install

```bash
git clone https://github.com/feynixai/acc.git && cd acc && cargo build --release && cp target/release/acc /usr/local/bin/
```

## Quick Start

```bash
# 1. Launch ACC in any project directory
cd ~/my-project
acc

# 2. Inside the TUI:
#    f  → add a folder (workspace)
#    a  → add an agent (spawns a pane with your command, e.g. "claude")
#    Enter → focus the selected agent's pane

# 3. From any terminal (while acc session is running):
acc list              # See all agents
acc read claude       # Read agent's terminal output
acc send claude "hi"  # Send text to an agent
acc root              # Launch root commander (orchestrator agent)
```

## CLI Commands

```
acc              Launch (or toggle sidebar if inside tmux)
acc kill         Close the sidebar
acc list         List all running agents with status
acc read <agent> Read agent's terminal output (last 50 lines)
acc send <agent> Send a prompt to an agent
acc root         Start root commander (orchestrator window)
acc help         Show help
```

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

## Root Commander

The root commander (`acc root`) is an orchestrator agent that manages other agents via CLI:

```bash
acc root    # Opens a ★ root window, starts claude with instructions
```

From the root commander, the agent can:
- `acc list` — discover all running agents
- `acc read <agent>` — check agent output
- `acc send <agent> "do X"` — delegate tasks

## Architecture

```
src/
├── main.rs      # Entry point, CLI args, tmux session bootstrap
├── lib.rs       # Module declarations
├── app.rs       # App state, tree model, input modes
├── cli.rs       # CLI subcommands (list, read, send, root)
├── event.rs     # Event loop (keys, mouse, tick)
├── handler.rs   # Core logic (sync, navigation, splits, undo)
├── command.rs   # Command mode (:), search (/), send (>)
├── ui.rs        # Rendering with ratatui
├── state.rs     # Persistent state (JSON), serde models
└── tmux.rs      # All tmux interactions, pane sync
```

### How it works

- **State is minimal**: Only folder names and paths are persisted to `~/.agent-center/workspaces/<dir>/state.json`. Agents are runtime-only — rebuilt from live tmux panes every 500ms tick.
- **Sidebar is a tmux pane**: ACC runs as a left-side pane (20% width) marked with the title `acc-sidebar`. When you switch folders, the sidebar physically moves between tmux windows via `join-pane`.
- **No duplication**: Agent lists are rebuilt from scratch each tick using `build_agents_from_panes()` — a pure function that maps tmux pane info to agent state.
- **Undo stack**: Deleting agents or folders pushes an entry onto the undo stack. `u` recreates the pane/window with the same command, working directory, and name.

### Per-workspace state

```
~/.agent-center/
└── workspaces/
    └── Users_arun_Desktop_BB/
        └── state.json
```

## License

MIT — [FeynixAI](https://github.com/feynixai)
