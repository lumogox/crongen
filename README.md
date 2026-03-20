# crongen

A cross-platform desktop application for scheduling, orchestrating, and monitoring autonomous coding agents. Build decision trees of AI agent tasks, fork competing approaches, auto-merge results, and watch it all execute through an interactive visual canvas with integrated terminals.

Built with **Tauri 2.x** (Rust backend + React frontend).

Source: [github.com/lumogox/crongen](https://github.com/lumogox/crongen)

![Dark theme, JetBrains Mono, visual execution graph]

---

## Table of Contents

- [Features](#features)
- [Supported Agents](#supported-agents)
- [Architecture](#architecture)
- [Getting Started](#getting-started)
- [Configuration](#configuration)
- [Core Concepts](#core-concepts)
- [Workflows](#workflows)
- [Git Integration](#git-integration)
- [Settings](#settings)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Development](#development)

---

## Features

- **Visual execution graph** -- React Flow canvas with auto-layout (dagre), drag-to-create nodes, linear and branching visualization modes
- **Multi-agent orchestration** -- Run Claude Code, Codex, Gemini, or custom shell agents in isolated git worktrees
- **Decision trees** -- Fork competing approaches at decision points, compare results, merge winners
- **Auto and supervised modes** -- Fully autonomous execution or pause-at-decision-points for human review
- **Integrated terminal** -- Live PTY output via xterm.js; pause/resume running agents with SIGSTOP/SIGCONT
- **Claude-backed plan generation** -- Describe a task in plain text and generate a structured execution plan (linear or branching)
- **Auto-merge with conflict resolution** -- Git merge conflicts are automatically resolved by Claude CLI using the configured execution model
- **Ship-it dialog** -- Merge preview with file list, commit count, step-by-step merge progress, and branch creation
- **TOON context system** -- Token-efficient structured context passed to each agent (ancestors, siblings, diffs)
- **Session management** -- Multiple sessions per project, each with its own node tree and execution history

---

## Supported Agents

| Agent | Execution Mode | Key Config |
|-------|---------------|------------|
| **Claude Code** | SDK (headless JSON stream) | model, max_turns, max_budget_usd, allowed/disallowed tools, skip_permissions |
| **Codex** | PTY (interactive terminal) | model, approval_mode (full-auto/suggest/auto-edit), sandbox, skip_git_check, json_output |
| **Gemini** | PTY (interactive terminal) | model, sandbox, yolo mode |
| **Custom** | PTY (interactive terminal) | configurable shell (bash/zsh/pwsh) |

### PTY Stdin Injection

TUI-based agents (Codex, Gemini, Custom) run in raw terminal mode. crongen handles interactive prompts automatically:

- **1500ms init delay** before first input (TUI startup time)
- **Split writes**: text and Enter (`\r`) sent as separate writes with 200ms gap (TUIs treat single `text\r` as paste)
- **Auto-responses**: Pattern-match PTY output and inject responses (e.g., auto-approve trust prompts, re-inject prompts after restarts)

### SDK Mode (Claude Code)

Claude Code runs headless with `--output-format stream-json`, capturing structured JSON-line output instead of raw terminal data. This enables richer progress tracking and tool-call visibility.

---

## Architecture

```
+------------------------------------------------------+
|                    Tauri Window                        |
|                                                       |
|  +------------------+  +---------------------------+  |
|  |    Sidebar        |  |    Decision Canvas        |  |
|  |  - Agent list     |  |    (React Flow + dagre)   |  |
|  |  - Session list   |  |  - ExecutionNode          |  |
|  |  - New task btn   |  |  - ExecutionEdge          |  |
|  |                   |  |  - NodePalette (DnD)      |  |
|  +------------------+  +---------------------------+  |
|                         +---------------------------+  |
|                         |   Inspector Panel          |  |
|                         |  - Node details            |  |
|                         |  - TerminalView (xterm.js) |  |
|                         |  - SDK session viewer      |  |
|                         +---------------------------+  |
+------------------------------------------------------+
         |  Tauri IPC (invoke / listen)  |
+------------------------------------------------------+
|                   Rust Backend                        |
|                                                       |
|  commands.rs -----> git_manager.rs                    |
|       |                  |                            |
|       +----> pty_manager.rs  (SIGSTOP/SIGCONT, PTY)  |
|       +----> sdk_manager.rs  (headless subprocess)    |
|       +----> orchestrator.rs (auto/supervised loop)   |
|       +----> plan_generator.rs (LLM plan creation)   |
|       +----> context.rs + toon.rs (agent context)     |
|       +----> db.rs (SQLite via rusqlite)              |
|                                                       |
+------------------------------------------------------+
```

### Event Flow

**Frontend to Backend** (Tauri `invoke`):
`create_agent`, `run_node`, `fork_node`, `merge_node_branch`, `start_orchestrator`, etc. (30+ commands)

**Backend to Frontend** (Tauri `emit`):
| Event | Payload | Purpose |
|-------|---------|---------|
| `session_started` | node_id | Node execution begins |
| `session_ended` | node_id, exit_code | Node execution completes |
| `pty_output` | session_id, data (base64) | Live terminal data |
| `sdk_output` | session_id, line (JSON) | Structured SDK output |
| `orchestrator_progress` | completed_count, total_count | Execution progress |
| `orchestrator_decision_needed` | pending_decision | User must choose at decision point |
| `orchestrator_complete` | success | Session finished |

---

## Getting Started

### Prerequisites

- [Bun](https://bun.sh/) (package manager)
- [Rust](https://rustup.rs/) (stable toolchain)
- [Tauri CLI](https://tauri.app/start/) (`cargo install tauri-cli`)
- At least one supported agent installed:
  - [Claude Code](https://docs.anthropic.com/en/docs/agents/claude-code) (`claude` CLI)
  - [Codex](https://github.com/openai/codex) (`codex` CLI)
  - [Gemini CLI](https://github.com/google-gemini/gemini-cli) (`gemini` CLI)

### Install & Run

```bash
# Clone the repository
git clone https://github.com/lumogox/crongen.git
cd crongen

# Install frontend dependencies
bun install

# Run in development mode (frontend + Rust backend)
bun run tauri dev
```

### Build for Distribution

```bash
bun run tauri build
```

This produces platform-specific installers in `src-tauri/target/release/bundle/`.

---

## Configuration

### Creating a Project (Agent)

1. Click **"New task"** in the header or **"+"** in the sidebar
2. Fill in:
   - **Name** -- project display name
   - **Repository path** -- local git repo (auto-initialized if empty)
   - **Agent type** -- Claude Code, Codex, Gemini, or Custom
   - **Type-specific config** -- model, permissions, sandbox, etc.
   - **Project mode** -- "Blank" (new repo) or "Existing" (use existing code)
3. Click Save

crongen will:
- Validate the repo path exists
- Initialize a git repo if needed (with `.crongen-worktrees` in `.gitignore`)
- Create an initial commit if the repo is empty

### Agent Type Configuration

**Claude Code:**
```
Model:              claude-sonnet-4-20250514 (default)
Max turns:          unlimited (optional limit)
Max budget (USD):   unlimited (optional cap)
Allowed tools:      comma-separated tool names (optional)
Disallowed tools:   comma-separated tool names (optional)
System prompt:      appended to default system prompt (optional)
Skip permissions:   bypass tool approval (dangerously_skip_permissions)
```

**Codex:**
```
Model:              codex-mini (default)
Approval mode:      full-auto / suggest / auto-edit
Sandbox:            enabled by default
Skip git check:     bypass clean-repo requirement
JSON output:        structured output mode
```

**Gemini:**
```
Model:              gemini-2.5-pro (default)
Sandbox:            enabled by default
YOLO mode:          skip all confirmations
```

**Custom:**
```
Shell:              bash / zsh / pwsh
```

---

## Core Concepts

### Decision Nodes

Every execution is organized as a tree of **decision nodes**. Each node has a type that determines its role:

| Type | Role | Executable? |
|------|------|:-----------:|
| **task** | Session root -- scaffolds or sets up the project | Yes |
| **decision** | Branch point -- groups competing approaches | No (structural) |
| **agent** | Worker -- executes a specific task/approach | Yes |
| **merge** | Conflict resolver -- reviews siblings, picks winner | Yes |
| **final** | Integration -- polishes the merged result | Yes |

### Branching Model

```
task (root)
  |
  decision
  /       \
agent-A   agent-B    <-- competing approaches, each in its own worktree
  \       /
   merge             <-- reviews diffs, picks winner, merges branch
     |
   final             <-- polishes result on merged branch
```

Each executable node runs in an **isolated git worktree** on its own branch (`crongen/{slug}/{timestamp}`). This prevents agents from stepping on each other's changes.

### TOON Context

Every agent receives structured execution context in **TOON format** (Token-Optimized Object Notation), which compresses ~40% vs JSON:

- **Session info** -- root label and goal
- **Ancestor chain** -- what ran before this node (root-first order)
- **Current node** -- this node's label, prompt, and type
- **Sibling info** -- status of parallel branches
- **Sibling diffs** -- git diffs from completed sibling branches (for merge nodes)
- **Parent diff** -- changes from parent branch (for final nodes)
- **Directive** -- orchestrator instructions (e.g., "pick the winning branch")

This context is wrapped in `<execution-context>` markers and injected into the agent's prompt.

### Orchestrator

The orchestrator runs a session's node tree in dependency order:

**Auto mode:**
- Executes nodes depth-first
- At decision points, runs all branches in parallel
- Merge nodes auto-run after all siblings complete
- Conflict resolution uses Claude (configurable model)

**Supervised mode:**
- Same as auto, but pauses at decision nodes
- Shows `OrchestratorDecisionModal` with options
- User selects winning branch before continuing
- Provides human oversight at critical decision points

---

## Workflows

### Quick Run

1. Create a project with a configured agent
2. Click **"New task"** -> enter a prompt -> **"Quick run"**
3. crongen creates a single root node and executes it immediately
4. Watch live output in the terminal panel

### Plan Generation

1. Click **"New task"** -> enter a complex prompt
2. Click **"Generate plan"** (optionally select linear/branching complexity)
3. An LLM generates a structured node tree (max 8 nodes)
4. Review the plan on the canvas -- edit labels/prompts as needed
5. Click **"Run all (Auto)"** or **"Supervised"** to execute

### Fork & Explore

1. Select a completed node on the canvas
2. Click **"Branch"** -> enter a new label and prompt
3. A child node is created on the same branch
4. Run it to explore an alternative approach
5. Compare results between siblings

### Merge to Main ("Ship It")

When all nodes in a session complete:

1. Header shows **"Session complete"** badge + **"Ship it"** button
2. Click "Ship it" to open the merge dialog
3. **Preview step**: see source branch, target branch, files changed, commit count
4. Choose **"Merge to main"** or **"Create feature branch"**
5. **Merging step**: animated stepper (auto-commit -> checkout -> merge -> resolve conflicts)
6. **Success step**: merge commit hash, auto-resolution summary if conflicts were resolved
7. **Conflict step** (if auto-resolution fails): full conflict file list + retry/branch options

---

## Git Integration

### Worktree Isolation

Each executable node gets its own git worktree:
```
your-repo/
  .crongen-worktrees/
    crongen-react-impl-1709000000/   <-- agent A's workspace
    crongen-vue-impl-1709000001/     <-- agent B's workspace
```

Worktrees are created from the parent node's commit, giving each agent a clean starting point. They're automatically cleaned up after a successful merge.

### Auto-Commit

Before merging, crongen auto-commits any uncommitted changes in the worktree. Agents don't always commit their work, so this ensures nothing is lost:

```
git add -A && git commit -m "Auto-commit agent work (uncommitted changes captured by crongen)"
```

### Merge with Fallback

The merge flow is resilient to edge cases:

1. **Worktree check**: skip auto-commit if the worktree directory no longer exists
2. **Branch fallback**: if the branch was deleted (e.g., by a previous merge cleanup), fall back to the node's commit hash -- `git merge <hash>` works even without a branch ref
3. **Conflict auto-resolution**: spawn Claude to resolve merge conflicts automatically
4. **Abort on failure**: if auto-resolution fails, abort the merge cleanly and present actionable options

### Default Branch Detection

The target branch for merges is detected automatically:
- Checks for `main` first, then `master`, then falls back to whatever HEAD points to

---

## Settings

Access via the gear icon in the header.

| Setting | Description | Default |
|---------|-------------|---------|
| **Debug mode** | Shows TOON context viewer in the inspector panel | Off |
| **Planning model** | LLM model for plan generation | (system default) |
| **Execution model** | LLM model for merge conflict resolution | haiku |

Settings are persisted in the SQLite database.

---

## Tech Stack

### Frontend

| Technology | Version | Purpose |
|-----------|---------|---------|
| React | 19 | UI framework |
| TypeScript | 5.x | Type safety |
| Vite | 6.x | Build tool & dev server |
| Tailwind CSS | 4 | Utility-first styling |
| @xyflow/react | 12 | Visual graph canvas |
| @dagrejs/dagre | - | Automatic tree layout |
| @xterm/xterm | 5 | Terminal emulation |
| shadcn/ui (Radix) | - | Accessible component primitives |
| lucide-react | 0.575 | Icons |
| date-fns | 4.1 | Date formatting |

### Backend

| Crate | Version | Purpose |
|-------|---------|---------|
| tauri | 2.10 | Desktop shell & IPC |
| rusqlite | 0.33 | SQLite database |
| portable-pty | 0.8 | Pseudo-terminal for agents |
| git2 | 0.19 | Git operations (worktrees, branches, merge) |
| tokio | 1 | Async runtime |
| toon-format | 0.4 | Token-efficient context serialization |
| uuid | 1 | ID generation |
| serde / serde_json | 1.0 | Serialization |

### Design System

- **Font**: JetBrains Mono (monospace throughout)
- **Root font size**: 13px
- **Theme**: Dark only (bg-base: `#0E1117`)
- **Design tokens**: sourced from `crongen.pen` design system
- **Node state colors**: running (green), completed (bright green), failed (red), paused (yellow), pending (gray), merged (purple)

---

## Project Structure

```
crongen/
├── src/                          # React frontend
│   ├── App.tsx                   # Root state management & event listeners
│   ├── main.tsx                  # Entry point
│   ├── index.css                 # Design tokens (@theme block)
│   ├── components/
│   │   ├── ui/                   # shadcn primitives (button, dialog, input, etc.)
│   │   ├── ContentArea.tsx       # 3-column layout (sidebar / canvas / inspector)
│   │   ├── DecisionCanvas.tsx    # React Flow canvas with dagre layout
│   │   ├── ExecutionNode.tsx     # Custom node renderer (status, actions)
│   │   ├── ExecutionEdge.tsx     # Custom edge renderer (color-coded)
│   │   ├── InspectorPanel.tsx    # Right panel (node details + terminal)
│   │   ├── TerminalView.tsx      # xterm.js terminal
│   │   ├── SdkSessionView.tsx    # Claude Code JSON output viewer
│   │   ├── Sidebar.tsx           # Agent & session list
│   │   ├── MergeDialog.tsx       # Ship-it merge workflow dialog
│   │   ├── AgentModal.tsx        # Create/edit agent form
│   │   ├── SessionModal.tsx      # New session / plan generation
│   │   ├── ForkModal.tsx         # Fork node dialog
│   │   ├── OrchestratorActivity.tsx        # Live orchestrator progress
│   │   ├── OrchestratorDecisionModal.tsx   # Decision point selection
│   │   ├── SettingsModal.tsx     # App settings
│   │   ├── NodePalette.tsx       # Drag-to-create node types
│   │   └── ...
│   ├── hooks/
│   │   ├── useTerminal.ts        # xterm.js + PTY event binding
│   │   ├── useSdkSession.ts      # SDK JSON stream binding
│   │   └── useTreeLayout.ts      # dagre layout computation
│   ├── lib/
│   │   ├── tauri-commands.ts     # Tauri IPC wrappers (70+ functions)
│   │   └── utils.ts              # Shared utilities
│   └── types/
│       ├── index.ts              # Domain types (mirrors Rust models)
│       └── node-types.ts         # Visual node type inference
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── lib.rs                # Tauri app setup & command registration
│   │   ├── models.rs             # Domain types (Agent, DecisionNode, etc.)
│   │   ├── db.rs                 # SQLite schema & CRUD operations
│   │   ├── commands.rs           # 30+ Tauri IPC command handlers
│   │   ├── agent_templates.rs    # CLI command builder per agent type
│   │   ├── orchestrator.rs       # Auto/supervised execution engine
│   │   ├── pty_manager.rs        # PTY lifecycle (spawn, pause, resume, kill)
│   │   ├── sdk_manager.rs        # Headless SDK execution (Claude Code)
│   │   ├── git_manager.rs        # Worktrees, merge, conflict resolution, preview
│   │   ├── context.rs            # Execution context builder (ancestors, diffs)
│   │   ├── toon.rs               # TOON format serialization
│   │   └── plan_generator.rs     # LLM-powered plan generation
│   ├── Cargo.toml
│   └── tauri.conf.json           # Window config, CSP, plugins
│
├── package.json
├── vite.config.ts
└── tsconfig.json
```

---

## Database Schema

Three tables in SQLite (stored in Tauri's app data directory):

### `agents`

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| name | TEXT | Display name |
| prompt | TEXT | Default prompt / description |
| shell | TEXT | Shell type (resolved from agent_type) |
| repo_path | TEXT | Absolute path to git repository |
| is_active | INTEGER | Enabled flag |
| agent_type | TEXT | claude_code / codex / gemini / custom |
| type_config | TEXT (JSON) | Agent-specific configuration |
| project_mode | TEXT | blank / existing |
| created_at | INTEGER | Unix timestamp |
| updated_at | INTEGER | Unix timestamp |

### `decision_nodes`

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| agent_id | TEXT FK | References agents(id) |
| parent_id | TEXT FK | References decision_nodes(id), NULL for root |
| label | TEXT | Short display label |
| prompt | TEXT | Agent instruction |
| branch_name | TEXT | Git branch (e.g., crongen/slug/timestamp) |
| worktree_path | TEXT | Absolute path to git worktree |
| commit_hash | TEXT | HEAD commit when created |
| status | TEXT | pending/running/paused/completed/failed/merged |
| exit_code | INTEGER | Process exit code (NULL while running) |
| node_type | TEXT | task/decision/agent/merge/final |
| scheduled_at | TEXT | ISO 8601 (only on root session nodes) |
| created_at | INTEGER | Unix timestamp |
| updated_at | INTEGER | Unix timestamp |

### `orchestrator_sessions`

| Column | Type | Description |
|--------|------|-------------|
| session_root_id | TEXT PK | References decision_nodes(id) |
| mode | TEXT | auto / supervised |
| state | TEXT | idle/running/waiting_user/complete/failed |
| current_node_id | TEXT | Currently executing node |
| started_at | INTEGER | Unix timestamp |
| updated_at | INTEGER | Unix timestamp |

---

## Development

### Commands

```bash
bun run dev          # Start Vite dev server (frontend only, port 1420)
bun run tauri dev    # Start full Tauri app (frontend + Rust backend)
bun run build        # Build frontend for production
bun run tauri build  # Build distributable app
```

### Type Checking

```bash
bunx tsc --noEmit    # TypeScript check
cargo check          # Rust check (from src-tauri/)
```

### Conventions

- Use `bun` for all JS/TS operations (not npm/yarn/pnpm)
- All Rust IPC commands return `Result<T, String>`
- Frontend communicates with backend via `@tauri-apps/api` `invoke()`
- Design tokens defined in `src/index.css` `@theme` block
- Dark theme only -- all UI assumes dark background
- Monospace font (JetBrains Mono) throughout

---

## Implementation Snapshot

The current codebase includes:
- Agent CRUD with per-agent config, active state, and project mode persisted in SQLite.
- React Flow decision trees with dagre auto-layout, drag-to-create structural nodes, and a right-side inspector/terminal panel.
- PTY sessions with xterm.js rendering, pause/resume support, stdin injection, and auto-response handling for interactive agents.
- Claude Code SDK sessions with `stream-json` output parsing and a dedicated session viewer.
- Git worktree isolation, branch naming, merge previews, merge fallback handling, and Claude-backed conflict resolution.
- Auto and supervised orchestrator modes with decision-point UI and progress events.
- Claude-backed plan generation for linear and branching execution trees.
- Settings persistence for debug mode, planning model, and execution model.

---

## License

MIT License. See [LICENSE](LICENSE).
