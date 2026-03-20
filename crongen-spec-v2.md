# crongen: Autonomous Scheduled Project Environment
**Version:** 1.0.0 (Revised)  
**Status:** Draft  
**Target Platforms:** Windows, macOS, Linux  
**Architecture:** Tauri 2.x (Rust backend + React/TypeScript frontend)

---

## Table of Contents

1. [Overview](#1-overview)
2. [Goals & Non-Goals](#2-goals--non-goals)
3. [Technology Stack](#3-technology-stack)
4. [System Architecture](#4-system-architecture)
5. [Data Models](#5-data-models)
6. [Backend Specification (Rust)](#6-backend-specification-rust)
7. [Frontend Specification (React/TypeScript)](#7-frontend-specification-reacttypescript)
8. [IPC Interface Contract](#8-ipc-interface-contract)
9. [State Management](#9-state-management)
10. [Error Handling Strategy](#10-error-handling-strategy)
11. [Security Considerations](#11-security-considerations)
12. [Shutdown & Lifecycle](#12-shutdown--lifecycle)
13. [Design Decisions](#13-design-decisions)
14. [Open Questions](#14-open-questions)

---

## 1. Overview

crongen is a cross-platform desktop application for defining, scheduling, and monitoring repository-bound coding projects. Users create projects that point at git repositories, and each project owns a decision tree that runs executable agent nodes inside isolated worktrees. At any point the user can fork a running or completed session — creating a new git worktree and branching the tree — to explore alternative approaches in parallel.

The primary use case is automating LLM CLI agent workflows (e.g., running `claude`, `aider`, or custom scripts) on a schedule, with full visual control over branching, comparison, and merging of agent-driven code changes.

---

## 2. Goals & Non-Goals

### Goals
- Define projects with a name, shell target, initial prompt, cron expression, timezone, and git repository path.
- Visualize each project's execution history as an interactive decision tree on a pannable, zoomable canvas.
- Support branching (forking) from any paused or completed node, backed by `git worktree` for filesystem isolation.
- Support merging a chosen branch back into the repository's main branch.
- Provide a split-view terminal panel (xterm.js) for observing or interacting with any active session by selecting its tree node.
- Reliably trigger project execution via `tokio-cron-scheduler` on all three target platforms.
- Persist project and decision tree configuration in a local SQLite database.
- Allow immediate, on-demand project execution outside of the cron schedule.
- Survive app restarts: re-register all active scheduled projects on startup.
- Gracefully shut down all PTY sessions on app exit.

### Non-Goals (v1.0.0)
- Remote/networked task execution.
- Session output persistence / searchable history (post-session logs).
- Multi-user or authentication support.
- Project dependencies or chaining.
- Built-in LLM API integration (the PTY shell handles this externally).

---

## 3. Technology Stack

| Layer | Technology | Version Target |
|---|---|---|
| Desktop Shell | Tauri | 2.x |
| Backend Language | Rust | stable (1.78+) |
| PTY Management | `portable-pty` | latest |
| Task Scheduling | `tokio-cron-scheduler` | latest |
| Local Database | `rusqlite` | latest (bundled feature) |
| Git Operations | `git2-rs` | latest |
| Async Runtime | Tokio | 1.x |
| Frontend Language | TypeScript | 5.x |
| UI Framework | React | 18.x |
| Build Tool | Vite | 5.x |
| Styling | Tailwind CSS | 3.x |
| Canvas / Node Graph | `@xyflow/react` (React Flow) | 12.x |
| Tree Layout Engine | `@dagrejs/dagre` | latest |
| Terminal Emulator | `xterm.js` + `xterm-addon-fit` | latest |

### Library Rationale

**React Flow (`@xyflow/react`):** The canvas and decision tree are the core UI surface. React Flow provides pan/zoom/select, custom node and edge rendering (nodes are plain React components styled with Tailwind), keyboard navigation, minimap, and fit-to-view — all out of the box under an MIT license. ~3M weekly npm downloads, 35K GitHub stars, actively maintained.

**dagre (`@dagrejs/dagre`):** Computes top-down tree layout positions. React Flow handles rendering; dagre handles where nodes go. Minimal configuration (`rankdir: 'TB'`, spacing options). If variable-height nodes or animated re-layout become requirements, dagre can be swapped for `elkjs` with the same integration pattern — only the layout function changes.

---

## 4. System Architecture

### 4.1 Component Diagram

```
┌──────────────────────────────────────────────────────────────┐
│                       Tauri Application                      │
│                                                              │
│  ┌────────────────────────┐   ┌───────────────────────────┐  │
│  │    React Frontend      │   │      Rust Backend         │  │
│  │                        │   │                           │  │
│  │  ┌──────────────────┐  │   │  ┌─────────────────────┐  │  │
│  │  │ Sidebar          │  │   │  │  db.rs              │  │  │
│  │  │ (Project List)   │  │   │  │  (SQLite CRUD)      │  │  │
│  │  └──────────────────┘  │   │  └─────────────────────┘  │  │
│  │                        │   │                           │  │
│  │  ┌──────────────────┐  │   │  ┌─────────────────────┐  │  │
│  │  │ DecisionCanvas   │◄─┼───┼──│  pty_manager.rs     │  │  │
│  │  │ (React Flow)     │  │   │  │  (portable-pty)     │  │  │
│  │  └──────────────────┘  │   │  └─────────────────────┘  │  │
│  │                        │   │                           │  │
│  │  ┌──────────────────┐  │   │  ┌─────────────────────┐  │  │
│  │  │ TerminalPanel    │  │   │  │  scheduler.rs       │  │  │
│  │  │ (xterm.js, split)│  │   │  │  (cron triggers)    │  │  │
│  │  └──────────────────┘  │   │  └─────────────────────┘  │  │
│  │                        │   │                           │  │
│  │  ┌──────────────────┐  │   │  ┌─────────────────────┐  │  │
│  │  │ ProjectModal     │  │   │  │  git_manager.rs     │  │  │
│  │  │ ForkModal        │  │   │  │  (worktree ops)     │  │  │
│  │  └──────────────────┘  │   │  └─────────────────────┘  │  │
│  │                        │   │                           │  │
│  │       Tauri IPC        │   │  ┌─────────────────────┐  │  │
│  │  Commands ─────────────┼───┼─►│  commands.rs        │  │  │
│  │  Events  ◄─────────────┼───┼──│  (IPC handlers)     │  │  │
│  └────────────────────────┘   └──┴─────────────────────┴──┘  │
└──────────────────────────────────────────────────────────────┘
```

### 4.2 Execution Flow

```
User defines project (ProjectModal)
        │
        ▼
create_project [IPC Command]
        │
        ├──► db.rs: INSERT into projects
        │
        └──► scheduler.rs: register job with tokio-cron-scheduler
                    │
             [Cron fires / run_project_now]
                    │
                    ├──► Check concurrency: is a session for this agent already running?
                    │       ├── YES → Log warning, skip execution
                    │       └── NO  → Continue
                    │
                    ▼
           git_manager.rs: create worktree from HEAD
                    │
                    ▼
           pty_manager.rs: spawn PTY pair in worktree dir (via spawn_blocking)
                    │
                    ├──► db.rs: INSERT root decision_node
                    │
                    ├──► Emit: session_started { session_id, node_id, project_id, project_name }
                    │              │
                    │              ▼
                    │     React: add node to canvas, auto-select, open terminal panel
                    │
                    ├──► Write agent.prompt + "\n" to PTY stdin
                    │
                    └──► Spawn blocking reader thread:
                              PTY stdout ──► Buffer (5ms / 16KB flush)
                                  ──► Emit: pty_output { session_id, data (base64) }
                                                │
                                                ▼
                                       React: decode + xterm.js.write(data)

[User selects node on canvas]
        │
        ▼
Terminal panel opens (split view) showing that node's session output

[User keystrokes in terminal panel]
        │
        ▼
write_pty [IPC Command] ──► pty_manager.rs: write to PTY stdin

[User forks from a paused/completed node]
        │
        ▼
fork_node [IPC Command]
        │
        ├──► git_manager.rs: git worktree add (from node's commit)
        ├──► db.rs: INSERT child decision_node
        ├──► pty_manager.rs: spawn PTY in new worktree
        └──► Emit: session_started (new node)
                    │
                    ▼
           React: add child node to tree, re-layout via dagre, auto-select

[User merges a completed branch]
        │
        ▼
merge_branch [IPC Command]
        │
        ├──► git_manager.rs: git merge branch → main
        ├──► git_manager.rs: git worktree remove
        ├──► db.rs: UPDATE node status = 'merged'
        └──► Emit: branch_merged { node_id }
                    │
                    ▼
           React: apply merge styling, dim sibling branches

[Process exits]
        │
        └──► pty_manager.rs: Emit: session_ended { session_id, node_id, exit_code }
                                        │
                                        ▼
                               React: update node status on canvas
```

---

## 5. Data Models

### 5.1 SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,            -- UUID v4
    name        TEXT NOT NULL,
    prompt      TEXT NOT NULL,               -- Initial stdin injection (root node prompt)
    shell       TEXT NOT NULL,               -- e.g. "bash", "pwsh", "zsh"
    repo_path   TEXT NOT NULL,               -- Absolute path to git repository
    cron_expr   TEXT NOT NULL,               -- Standard 5-field cron
    timezone    TEXT NOT NULL DEFAULT 'LOCAL', -- IANA identifier or 'LOCAL' for system tz
    is_active   INTEGER NOT NULL DEFAULT 1,  -- 0 = disabled, 1 = enabled
    created_at  INTEGER NOT NULL,            -- Unix timestamp (seconds)
    updated_at  INTEGER NOT NULL             -- Unix timestamp (seconds)
);

CREATE INDEX IF NOT EXISTS idx_projects_active ON projects(is_active);

CREATE TABLE IF NOT EXISTS decision_nodes (
    id              TEXT PRIMARY KEY,        -- UUID v4
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES decision_nodes(id), -- NULL for root
    label           TEXT NOT NULL,
    prompt          TEXT NOT NULL,           -- The prompt/command injected for this branch
    branch_name     TEXT NOT NULL,           -- Git branch name
    worktree_path   TEXT,                    -- Filesystem path to worktree (NULL after cleanup)
    commit_hash     TEXT,                    -- Git commit hash at this node's state
    status          TEXT NOT NULL DEFAULT 'pending',
        -- pending | running | paused | completed | failed | merged
    exit_code       INTEGER,                -- NULL until process exits
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_nodes_project ON decision_nodes(project_id);
CREATE INDEX IF NOT EXISTS idx_nodes_parent ON decision_nodes(parent_id);
```

### 5.2 Rust Domain Types

```rust
// src-tauri/src/models.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub shell: String,
    pub repo_path: String,
    pub cron_expr: String,
    pub timezone: String,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecisionNode {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub label: String,
    pub prompt: String,
    pub branch_name: String,
    pub worktree_path: Option<String>,
    pub commit_hash: Option<String>,
    pub status: NodeStatus,
    pub exit_code: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Merged,
}
```

### 5.3 TypeScript Domain Types

```typescript
// src/types/index.ts

export interface Project {
  id: string;
  name: string;
  prompt: string;
  shell: string;
  repo_path: string;
  cron_expr: string;
  timezone: string;
  is_active: boolean;
  created_at: number;
  updated_at: number;
}

export type NodeStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'merged';

export interface DecisionNode {
  id: string;
  project_id: string;
  parent_id: string | null;
  label: string;
  prompt: string;
  branch_name: string;
  worktree_path: string | null;
  commit_hash: string | null;
  status: NodeStatus;
  exit_code: number | null;
  created_at: number;
  updated_at: number;
}

// React Flow node data shape (passed to custom node component)
export interface DecisionNodeData {
  node: DecisionNode;
  isSelected: boolean;
  onFork: (nodeId: string) => void;
  onMerge: (nodeId: string) => void;
}
```

---

## 6. Backend Specification (Rust)

### 6.1 `db.rs` — Database Layer

**Responsibilities:** SQLite initialization and all CRUD for `projects` and `decision_nodes`.

```
db_init(conn: &Connection) -> Result<()>
    Creates tables and indexes if they do not exist.

-- Project CRUD --
project_create(conn: &Connection, project: &Project) -> Result<()>
project_get_all(conn: &Connection) -> Result<Vec<Project>>
project_get_by_id(conn: &Connection, id: &str) -> Result<Project>
project_update(conn: &Connection, project: &Project) -> Result<()>
project_delete(conn: &Connection, id: &str) -> Result<()>

-- Decision Node CRUD --
node_create(conn: &Connection, node: &DecisionNode) -> Result<()>
node_get_tree(conn: &Connection, project_id: &str) -> Result<Vec<DecisionNode>>
    Returns all nodes for a project as a flat list. The frontend builds the tree.
node_get_by_id(conn: &Connection, id: &str) -> Result<DecisionNode>
node_update_status(conn: &Connection, id: &str, status: NodeStatus, exit_code: Option<i32>) -> Result<()>
node_update_commit(conn: &Connection, id: &str, commit_hash: &str) -> Result<()>
node_delete_branch(conn: &Connection, id: &str) -> Result<()>
    Deletes the node and all descendants (CASCADE via recursive CTE).
```

**Threading model:** The `Connection` is wrapped in `Arc<Mutex<Connection>>` and held in Tauri's managed state. **All DB calls must run inside `tokio::task::spawn_blocking`** to avoid holding a mutex guard across `.await` boundaries.

`db_init` is called once at `app.setup()`.

### 6.2 `git_manager.rs` — Git Worktree Operations

**Responsibilities:** All git operations — worktree creation/removal, branch management, merging. Uses `git2-rs` for programmatic access.

```rust
pub struct GitManager;

impl GitManager {
    pub fn validate_repo(path: &str) -> Result<()>
        // Confirms path is a valid git repository.

    pub fn create_worktree(
        repo_path: &str,
        branch_name: &str,
        from_commit: Option<&str>,  // None = HEAD
    ) -> Result<WorktreeInfo>
        // 1. Open repo at repo_path.
        // 2. Resolve from_commit (or HEAD).
        // 3. Create branch branch_name at that commit.
        // 4. Run git worktree add at a deterministic path:
        //    {repo_path}/../.crongen-worktrees/{branch_name}
        // 5. Return WorktreeInfo { worktree_path, commit_hash, branch_name }.

    pub fn remove_worktree(repo_path: &str, worktree_path: &str) -> Result<()>
        // Prune the worktree and delete the branch if it was merged.

    pub fn merge_branch(
        repo_path: &str,
        branch_name: &str,
        target: &str,  // e.g. "main"
    ) -> Result<MergeResult>
        // 1. Checkout target branch in main worktree.
        // 2. Merge branch_name into target.
        // 3. Return MergeResult { success, conflict_files, merge_commit_hash }.

    pub fn get_current_commit(worktree_path: &str) -> Result<String>
        // Returns the HEAD commit hash of the given worktree.
}

pub struct WorktreeInfo {
    pub worktree_path: String,
    pub commit_hash: String,
    pub branch_name: String,
}

pub struct MergeResult {
    pub success: bool,
    pub conflict_files: Vec<String>,
    pub merge_commit_hash: Option<String>,
}
```

**Worktree path convention:** Worktrees are stored in a sibling directory to the repo to keep the repo's own directory clean:
```
~/repos/myproject/                    ← The repo
~/repos/.crongen-worktrees/       ← Worktree storage
    fix-approach-a/
    fix-approach-b/
    fix-approach-b-alt/
```

### 6.3 `scheduler.rs` — Cron Scheduler

**Responsibilities:** Register/unregister cron jobs. Bridge cron triggers to `git_manager` + `pty_manager`. Enforce concurrency policy.

```
scheduler_init(db: Arc<Mutex<Connection>>, pty_mgr: Arc<PtyManager>)
    -> Result<JobScheduler>
    Loads all projects where is_active = 1 and registers each.

scheduler_add_project(sched: &JobScheduler, project: &Project,
                    pty_mgr: Arc<PtyManager>) -> Result<Uuid>
    Registers a single job. Stores the returned job UUID mapped to project.id.

scheduler_remove_project(sched: &JobScheduler, job_uuid: Uuid) -> Result<()>
```

- Job UUIDs are maintained in a `HashMap<String, Uuid>` (`project_id` → `job_uuid`) inside a `Mutex`.
- On trigger, the job closure: (1) checks concurrency, (2) creates a worktree via `git_manager`, (3) creates a root `DecisionNode`, (4) calls `pty_mgr.spawn_session(...)`.

**Timezone handling:**
- Each project stores a `timezone` field (IANA identifier or `LOCAL`).
- At evaluation time, "now" is converted to the project's timezone before matching against the cron expression.
- **DST edge cases:** Spring forward → skip. Fall back → run once (first occurrence).

**Concurrency policy (skip-if-running):** Before spawning, check `pty_manager.has_active_session_for_project(project_id)`. If active, skip and log a warning.

### 6.4 `pty_manager.rs` — PTY Lifecycle

**Responsibilities:** Spawn and manage PTY pairs. Route I/O between PTY and the Tauri event bus. Handle output batching.

**Internal State:**
```rust
pub struct PtyManager {
    sessions: Mutex<HashMap<String, ActiveSession>>,
}

struct ActiveSession {
    project_id: String,
    node_id: String,
    master: Box<dyn MasterPty + Send>,  // Keeps PTY alive; used for resize
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send>,       // Child process handle for shutdown
}
```

> **Critical:** The `MasterPty` handle must be retained for the lifetime of the session. If dropped, the PTY file descriptors close and the child process receives `SIGHUP`. The `Child` handle is retained for graceful shutdown (see §12).

**Public API:**
```
PtyManager::spawn_session(
    project: &Project,
    node: &DecisionNode,
    worktree_path: &str,
    app: AppHandle,
) -> Result<String>
    1. Validate that the shell exists on the system.
    2. Generate session_id (UUID v4).
    3. Spawn PTY pair via portable-pty with the target shell.
       Set working directory to worktree_path.
    4. Emit `session_started` event with { session_id, node_id, project_id, project_name }.
    5. Spawn a reader thread via `tokio::task::spawn_blocking`
       (see "Output pipeline" below).
    6. Write node.prompt + "\n" to PTY stdin.
    7. Insert ActiveSession into sessions map.
    8. Return session_id.

PtyManager::write(session_id: &str, data: &[u8]) -> Result<()>

PtyManager::resize(session_id: &str, cols: u16, rows: u16) -> Result<()>
    Calls master.resize(PtySize { rows, cols, .. }) on the stored MasterPty.

PtyManager::pause(session_id: &str) -> Result<()>
    Sends SIGTSTP (Unix) or suspends process (Windows) to the child.

PtyManager::resume(session_id: &str) -> Result<()>
    Sends SIGCONT (Unix) or resumes process (Windows).

PtyManager::has_active_session_for_project(project_id: &str) -> bool

PtyManager::shutdown_all(&self) -> ()
    Iterates all sessions, sends SIGTERM/TerminateProcess, waits, then force-kills.

PtyManager::cleanup_session(session_id: &str)
```

**Output pipeline (reader thread):**

Runs in `spawn_blocking` because `portable-pty` provides synchronous `Read`.

1. Read raw bytes from PTY stdout into a buffer.
2. Flush when buffer exceeds **16KB** or **5ms** elapse since last flush (whichever first).
3. If no new data arrives within 5ms, flush immediately (keeps interactive sessions responsive).
4. **Encode payload as base64** before emitting (PTY output is not guaranteed UTF-8).
5. On EOF / read error, capture exit code, emit `session_ended`, update node status in DB, call `cleanup_session`.

### 6.5 `commands.rs` — IPC Command Handlers

All commands are `#[tauri::command]` async functions.

```rust
// --- Project CRUD ---

#[tauri::command]
async fn create_project(
    state: State<'_, AppState>,
    name: String, prompt: String, shell: String,
    repo_path: String, cron_expr: String, timezone: String,
) -> Result<Project, String>
    // Validates cron_expr, shell, and repo_path (must be valid git repo).

#[tauri::command]
async fn update_project(state: State<'_, AppState>, project: Project) -> Result<Project, String>
    // Validates, updates DB, re-registers scheduler if cron/timezone/is_active changed.

#[tauri::command]
async fn get_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String>

#[tauri::command]
async fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String>
    // Removes from DB (cascades to decision_nodes) and scheduler.
    // Does NOT kill running sessions — they complete independently.

#[tauri::command]
async fn toggle_project(state: State<'_, AppState>, id: String, is_active: bool) -> Result<(), String>

// --- Decision Tree ---

#[tauri::command]
async fn get_decision_tree(state: State<'_, AppState>, project_id: String) -> Result<Vec<DecisionNode>, String>
    // Returns flat list of all nodes for the project. Frontend builds the tree.

#[tauri::command]
async fn run_project_now(
    state: State<'_, AppState>, app: AppHandle, id: String,
) -> Result<String, String>  // Returns session_id
    // Creates worktree from HEAD, inserts root node, spawns PTY.
    // Respects concurrency policy.

#[tauri::command]
async fn fork_node(
    state: State<'_, AppState>, app: AppHandle,
    node_id: String, label: String, prompt: String, start_immediately: bool,
) -> Result<DecisionNode, String>
    // 1. Look up parent node's commit_hash and project's repo_path.
    // 2. git_manager::create_worktree from that commit.
    // 3. Insert child DecisionNode in DB.
    // 4. If start_immediately: spawn PTY in new worktree.
    // 5. Return the new node.

#[tauri::command]
async fn merge_branch(
    state: State<'_, AppState>, node_id: String,
) -> Result<MergeResult, String>
    // 1. git_manager::merge_branch into main.
    // 2. On success: update node status to 'merged', remove worktree.
    // 3. Return MergeResult (success/conflict info).

#[tauri::command]
async fn delete_branch(
    state: State<'_, AppState>, node_id: String,
) -> Result<(), String>
    // Remove worktree, delete node + descendants from DB.

// --- PTY I/O ---

#[tauri::command]
async fn write_pty(state: State<'_, AppState>, session_id: String, data: String) -> Result<(), String>

#[tauri::command]
async fn resize_pty(state: State<'_, AppState>, session_id: String, cols: u16, rows: u16) -> Result<(), String>

#[tauri::command]
async fn pause_session(state: State<'_, AppState>, session_id: String) -> Result<(), String>
    // SIGTSTP the child. Update node status to 'paused'.

#[tauri::command]
async fn resume_session(state: State<'_, AppState>, session_id: String) -> Result<(), String>
    // SIGCONT the child. Update node status to 'running'.
```

**Shell validation:** On `create_project` and `update_project`, resolve the shell path and return a descriptive error if not found.

**Repo validation:** On `create_project` and `update_project`, call `GitManager::validate_repo` and return an error if the path is not a valid git repository.

**`AppState` struct:**
```rust
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub scheduler: Arc<Mutex<JobScheduler>>,
    pub pty_manager: Arc<PtyManager>,
    pub job_map: Arc<Mutex<HashMap<String, Uuid>>>,  // project_id -> cron job_uuid
}
```

---

## 7. Frontend Specification (React/TypeScript)

### 7.1 Component Tree

```
App
├── Sidebar
│   ├── ProjectListItem (×n)
│   └── [+ New Project] Button
├── ContentArea
│   ├── Toolbar (project name, zoom, fork/merge actions)
│   ├── DecisionCanvas (React Flow)
│   │   ├── ReactFlow
│   │   │   ├── DecisionNodeComponent (custom node, ×n)
│   │   │   ├── DecisionEdge (custom edge, ×n)
│   │   │   ├── MiniMap
│   │   │   ├── Controls
│   │   │   └── Background
│   │   └── useTreeLayout (dagre integration hook)
│   └── TerminalPanel (conditional, split right)
│       ├── TerminalHeader
│       ├── TerminalView (xterm.js)
│       └── NodeActions (fork, merge, pause/resume)
├── ProjectModal (conditional)
├── ForkModal (conditional)
├── StatusBar
└── ToastContainer
```

### 7.2 Canvas Integration — React Flow + dagre

**`DecisionCanvas` component:**

Wraps `<ReactFlow>` and owns the node/edge state derived from the `DecisionNode[]` fetched via `get_decision_tree`.

```typescript
// Simplified structure — not complete implementation code

import { ReactFlow, useNodesState, useEdgesState, Background, Controls, MiniMap } from '@xyflow/react';
import dagre from '@dagrejs/dagre';

// Register custom node type
const nodeTypes = { decision: DecisionNodeComponent };

// Register custom edge type
const edgeTypes = { decisionEdge: DecisionEdge };

function DecisionCanvas({ tree, selectedNodeId, onSelectNode }: Props) {
  // Convert DecisionNode[] to React Flow nodes + edges
  // Run dagre layout to compute positions
  // Render ReactFlow with custom nodes, edges, background, minimap
}
```

**Layout computation (`useTreeLayout` hook):**

Encapsulates the dagre integration. Called whenever the tree changes (node added, status changed, branch deleted).

```typescript
function useTreeLayout(tree: DecisionNode[]): { nodes: Node[], edges: Edge[] } {
  // 1. Create dagre graph with rankdir: 'TB' (top-to-bottom).
  // 2. Set node spacing: ranksep (vertical gap between parent/child) = 80px,
  //    nodesep (horizontal gap between siblings) = 40px.
  // 3. For each DecisionNode: add to dagre with measured width/height.
  //    Node width: 200px fixed. Height: variable (60-80px based on content).
  // 4. For each node with parent_id: add edge from parent to child.
  // 5. Run dagre.layout().
  // 6. Map dagre output to React Flow node positions and edge definitions.
  // 7. Return { nodes, edges }.
}
```

**Re-layout triggers:**
- New node added (fork, initial run).
- Node deleted (branch deletion).
- Project switch (sidebar selection).
- NOT on status change alone (node stays in place, only visual style updates).

**`DecisionNodeComponent` (custom React Flow node):**

A React component registered via `nodeTypes`. Receives `DecisionNodeData` via the `data` prop. Renders the node card described in the UI Design Spec §6.3: label, commit hash, prompt preview, status indicator. Styled with Tailwind. Emits selection on click (React Flow handles this natively via `onNodeClick`).

**`DecisionEdge` (custom React Flow edge):**

Bezier curve edge. Default color: `border-default`. When the edge is on the path from root to the selected node: `accent` blue. Merged edges: `merge-highlight` purple.

**Canvas features used from React Flow:**
- `fitView` — maps to toolbar "Fit All" button.
- `MiniMap` — bottom-right corner overview of large trees.
- `Controls` — zoom in/out/fit buttons (restyled to match dark theme).
- `Background` — dot pattern using `bg-base` / `text-muted` for subtle grid.
- `onNodeClick` — triggers node selection → opens terminal panel.
- `onPaneClick` — deselects node → closes terminal panel.
- Keyboard: arrow keys between nodes (React Flow's built-in a11y).

### 7.3 Component Contracts

#### `App`
- Fetches initial project list via `get_projects` on mount.
- Holds top-level state: `projects`, `selectedProjectId`, `tree` (for the selected project), `selectedNodeId`, `isTerminalOpen`, `modals`.
- Registers global Tauri event listeners on mount; unlistens on unmount.
- When `selectedProjectId` changes, fetches that project's tree via `get_decision_tree`.

#### `Sidebar`
**Props:** `projects`, `selectedProjectId`, `onSelectProject`, `onNewProject`, `onEditProject`, `onDeleteProject`, `onRunNow`, `onToggle`

Each `ProjectListItem` displays: name, repo path (truncated), shell badge, cron expression, timezone, branch count + running count, active toggle, overflow menu.

#### `TerminalPanel`
**Props:** `node: DecisionNode`, `onClose: () => void`

- Slides in from the right, 45% width (draggable divider, min 30%, max 70%).
- Header: status dot, node label, commit hash, worktree path, close button.
- Action bar (contextual):
  - Running: Pause button.
  - Paused: Resume, Fork from here.
  - Completed: Fork from here, Merge this branch, View Diff.
  - Failed: Fork from here, View Diff.
- Terminal area: xterm.js instance scoped to this node's session.
- Subscribes to `pty_output` events filtered by `session_id`, decodes base64, writes to xterm.
- On `terminal.onData`, invokes `write_pty`.
- On fit/resize, invokes `resize_pty`.

#### `ProjectModal`
**Props:** `project: Project | null` (null = create), `onClose`, `onSave`

Fields: Name, Repository Path (with folder picker + validation), Shell (select), Initial Prompt (textarea), Cron Expression (with validation + human-readable preview), Timezone (searchable select, defaults to LOCAL).

#### `ForkModal`
**Props:** `parentNode: DecisionNode`, `onClose`, `onFork`

Fields: Branch Name (auto-generated, editable), New Prompt (textarea, required), Start Immediately (checkbox, default checked).

### 7.4 Tauri Event Handling

```typescript
// On mount in App
const unlistenStarted = await listen<SessionStartedPayload>('session_started', (event) => {
  const { session_id, node_id, project_id, project_name } = event.payload;
  // Update the node's status to 'running' in the tree state.
  // If this project is currently selected, the canvas re-renders the node.
  // Auto-select the new node → terminal panel opens.
});

const unlistenEnded = await listen<SessionEndedPayload>('session_ended', (event) => {
  const { session_id, node_id, exit_code } = event.payload;
  // Update node status to 'completed' or 'failed' based on exit_code.
});

const unlistenMerged = await listen<BranchMergedPayload>('branch_merged', (event) => {
  // Update node status to 'merged'. Dim sibling branches on canvas.
});
```

`pty_output` events are handled inside `TerminalPanel` (scoped to the active session).

---

## 8. IPC Interface Contract

### 8.1 Commands (Frontend → Backend)

| Command | Parameters | Return | Description |
|---|---|---|---|
| `get_projects` | — | `Project[]` | Fetch all projects |
| `create_project` | `name, prompt, shell, repo_path, cron_expr, timezone` | `Project` | Validate, persist, schedule |
| `update_project` | `Project` (full object) | `Project` | Validate, update, re-register scheduler |
| `delete_project` | `id` | `void` | Remove from DB (cascade) + scheduler |
| `toggle_project` | `id, is_active` | `void` | Enable/disable scheduling |
| `get_decision_tree` | `project_id` | `DecisionNode[]` | Flat list of all nodes for project |
| `run_project_now` | `id` | `session_id` | Create root node + worktree, spawn PTY |
| `fork_node` | `node_id, label, prompt, start_immediately` | `DecisionNode` | Create child worktree + node, optionally spawn |
| `merge_branch` | `node_id` | `MergeResult` | Merge into main, cleanup worktree |
| `delete_branch` | `node_id` | `void` | Remove worktree + node + descendants |
| `write_pty` | `session_id, data` | `void` | Forward keystrokes |
| `resize_pty` | `session_id, cols, rows` | `void` | Update PTY dimensions |
| `pause_session` | `session_id` | `void` | SIGTSTP child process |
| `resume_session` | `session_id` | `void` | SIGCONT child process |

All commands return `Result<T, String>`.

### 8.2 Events (Backend → Frontend)

| Event | Payload | Description |
|---|---|---|
| `session_started` | `{ session_id, node_id, project_id, project_name }` | PTY ready; update node, open terminal |
| `pty_output` | `{ session_id, data }` | Base64-encoded raw PTY output chunk |
| `session_ended` | `{ session_id, node_id, exit_code }` | Process terminated; update node status |
| `branch_merged` | `{ node_id, merge_commit_hash }` | Merge complete; update tree styling |

---

## 9. State Management

No external state library for v1.0.0. State lives in `App` via `useReducer`.

### Top-Level State Shape

```typescript
interface AppState {
  projects: Project[];
  selectedProjectId: string | null;
  tree: DecisionNode[];              // Flat list for the selected project
  selectedNodeId: string | null;     // Currently selected node on canvas
  isTerminalOpen: boolean;
  modals: {
    project: { open: boolean; editing: Project | null };
    fork: { open: boolean; parentNode: DecisionNode | null };
  };
}
```

### State Transitions

| Action | State Change |
|---|---|
| `get_projects` resolves | `projects = result` |
| `create_project` resolves | `projects = [...projects, new]`, close modal |
| `update_project` resolves | Replace project in list, close modal |
| `delete_project` resolves | Remove project from list, clear selection if deleted |
| Select project in sidebar | `selectedProjectId = id`, fetch tree |
| `get_decision_tree` resolves | `tree = result` |
| `session_started` event | Update matching node in tree: `status = 'running'`, auto-select |
| `session_ended` event | Update matching node: `status = completed/failed`, `exit_code` |
| `fork_node` resolves | `tree = [...tree, newNode]` (triggers dagre re-layout) |
| `merge_branch` resolves | Update node: `status = 'merged'` |
| `delete_branch` resolves | Remove node + descendants from tree (triggers re-layout) |
| Click node on canvas | `selectedNodeId = id`, `isTerminalOpen = true` |
| Click canvas background / Esc | `selectedNodeId = null`, `isTerminalOpen = false` |
| Project deleted while session active | No change to running sessions |

---

## 10. Error Handling Strategy

### Backend
- All public functions return `Result<T, anyhow::Error>`.
- `commands.rs` maps errors to `Result<T, String>` (`.map_err(|e| e.to_string())`).
- PTY spawn failures: log error, emit synthetic `session_ended` with `exit_code: -1`.
- Shell validation errors: descriptive message at project creation/update time.
- Repo validation errors: descriptive message ("Not a git repository" / "Path does not exist").
- Git worktree failures: return error to frontend, do not leave partial state. If worktree was created but PTY fails to spawn, clean up the worktree.
- Merge conflicts: return `MergeResult { success: false, conflict_files }`. The user resolves via the terminal panel (interactive PTY in the worktree).
- Scheduler registration failures: log, don't crash. Project remains in DB; user can retry.
- Concurrency violations: return descriptive error.

### Frontend
- `invoke()` rejections display a non-blocking toast notification.
- `TerminalPanel` is wrapped in an `ErrorBoundary`.
- Nodes with `status === 'failed'` display red border and warning icon on the canvas.
- Merge conflict results surface a toast + the terminal panel shows the worktree for manual resolution.

---

## 11. Security Considerations

- **Shell injection:** The `prompt` field is injected directly into PTY stdin. No sanitization — intentionally a power-user tool. Users execute arbitrary shell input.
- **Shell validation:** Shell path is resolved at project creation (UX guard, not security boundary).
- **Repo validation:** `repo_path` is validated as a git repository. The app does not restrict which repos can be used.
- **Worktree isolation:** Each branch runs in its own worktree. Concurrent branches cannot corrupt each other's filesystem state.
- **No network exposure:** Tauri local IPC only. No HTTP server.
- **Tauri CSP:** Strict Content Security Policy in `tauri.conf.json`.
- **File system access:** Scoped to app data directory for the DB. Worktree operations use the user-specified repo path.
- **Cron expression validation:** Client-side + server-side before passing to `tokio-cron-scheduler`.

---

## 12. Shutdown & Lifecycle

### App Startup
1. Initialize SQLite connection and run `db_init`.
2. Initialize `PtyManager` (empty sessions map).
3. Initialize `JobScheduler` via `scheduler_init`: load all `is_active = 1` projects and register cron jobs.
4. NOTE: Worktrees from previous sessions may still exist on disk. On startup, scan `decision_nodes` for nodes with `status = 'running'` or `'paused'` and set them to `'failed'` (the PTY process is gone). Worktrees are left on disk for user inspection; they can be cleaned up via `delete_branch`.

### App Shutdown

Register a handler on `tauri::RunEvent::ExitRequested`:

1. Call `pty_manager.shutdown_all()`.
2. For each active session:
   a. Send `SIGTERM` (Unix) or `TerminateProcess` (Windows).
   b. Wait up to 3 seconds for graceful exit.
   c. If still running, `SIGKILL` / hard-kill.
   d. Drop `MasterPty` handle, remove from sessions map.
   e. Update node status to `'failed'` in DB (process was interrupted).
3. Shut down the `JobScheduler`.
4. Close the SQLite connection.

### Project Deletion While Session Active

Deleting a project cascades to all its `decision_nodes` in the DB. Running sessions from that project continue to completion independently (the PTY is already spawned). Worktrees on disk are NOT automatically cleaned up on project deletion — a cleanup command or manual removal is needed.

---

## 13. Design Decisions

### #1 — PTY output batching
Adaptive batching in the Rust reader thread: 5ms debounce, 16KB max batch, flush on idle. See §6.4.

### #2 — Session output persistence
Deferred to v1.1. Output pipeline designed for easy tee-to-file addition.

### #3 — Maximum concurrent PTY sessions
Soft cap of 20 (warning), hard cap of 50 (blocked). Per-project concurrency: skip-if-running.

### #4 — Separate commands for create/update
Separate `create_project` and `update_project`. Separate `fork_node` for tree branching.

### #5 — Timezone handling
Store `timezone` alongside `cron_expr`. Default to `LOCAL`. Skip on spring-forward, run once on fall-back.

### #6 — Auto-launch on system startup
Off by default. Opt-in via settings. Start minimized when enabled.

### #7 — Canvas library
React Flow (`@xyflow/react`) for the canvas + dagre (`@dagrejs/dagre`) for tree layout. React Flow provides pan/zoom/select/custom nodes/keyboard navigation. Dagre computes top-down positions. Both MIT-licensed, lightweight, and broadly adopted. dagre can be swapped for elkjs later if variable-height nodes or animated re-layout require it.

### #8 — Git worktree strategy
One repo per project. Worktrees stored in a sibling directory (`../.crongen-worktrees/`). `git2-rs` for all git operations. Merge back into main when the user picks a winning branch. Conflict resolution happens interactively in the terminal panel.

---

## 14. Open Questions

| # | Question | Owner | Priority |
|---|---|---|---|
| 1 | Should `concurrency_policy` be per-agent (skip / queue / allow-parallel)? | Product | Low (v1.1) |
| 2 | Should completed session output be persisted for later review? | Product | Low (v1.1) |
| 3 | Are session caps (20/50) appropriate? Profile on target hardware? | Backend | Medium |
| 4 | Should the app auto-launch on startup? | Product | Low (v1.1) |
| 5 | Should orphaned worktrees be auto-cleaned on startup, or only via explicit user action? | Backend | Medium |
| 6 | Should `git2-rs` be used exclusively, or shell out to `git` CLI for operations like merge that benefit from user's git config (merge strategies, hooks)? | Backend | High |
| 7 | How should the canvas handle very deep trees (50+ nodes)? Collapse subtrees? Virtual rendering? | Frontend | Medium |
