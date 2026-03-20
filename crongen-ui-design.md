# crongen — UI Design Specification
**Companion to:** crongen Spec v1.0.0 (Revised)  
**Paradigm:** Canvas-based decision tree with git worktree branching

---

## 1. Design Principles

**The tree is the product.** The canvas showing the agent's decision tree is the primary artifact users interact with. Every design choice supports reading, navigating, and steering the tree.

**Spatial over temporal.** Traditional terminal apps present agent output as a timeline (scrollback). crongen presents it as a *map* of explored paths. The user thinks in branches, not in history.

**Calm by default, loud on exceptions.** The interface is visually quiet during normal operation. Color and motion are reserved for state changes: a branch completing, a fork point ready, an error surfacing.

**Keyboard-first, mouse-friendly.** The canvas supports keyboard navigation between nodes. The terminal panel captures all input when focused.

---

## 2. Core Concepts

### Agent
A configured task (name, shell, prompt, cron schedule) bound to a **single git repository**. The agent operates within worktrees of that repo. One agent = one repo = one decision tree.

### Decision Node
A point in the tree representing a specific state: a git commit + prompt + session output. Nodes have a lifecycle: `pending → running → paused | completed | failed`.

### Branch
A path from the root to a leaf. Each branch corresponds to a git worktree on disk. Multiple branches can run simultaneously because worktrees provide filesystem isolation.

### Fork
The act of creating a new branch from an existing node. The backend runs `git worktree add` from that node's commit. The user provides a new prompt or instruction for the forked path.

### Merge
The user selects a "winning" branch and merges it back into the main branch of the repo. This is the terminal action for a decision tree (or a subtree).

---

## 3. Layout

### 3.1 Default State (no node selected)

```
┌──────────────────────────────────────────────────────────────────┐
│  Title Bar                                                       │
├────────────┬─────────────────────────────────────────────────────┤
│            │  Toolbar (agent name, zoom, actions)                │
│            ├─────────────────────────────────────────────────────┤
│  Sidebar   │                                                     │
│  (260px)   │                                                     │
│            │                Canvas                               │
│  Agent     │             (Decision Tree)                         │
│  List      │                                                     │
│            │                                                     │
│            │                                                     │
├────────────┴─────────────────────────────────────────────────────┤
│  Status Bar                                                      │
└──────────────────────────────────────────────────────────────────┘
```

### 3.2 Node Selected (split view)

```
┌──────────────────────────────────────────────────────────────────┐
│  Title Bar                                                       │
├────────────┬──────────────────────────┬──────────────────────────┤
│            │  Toolbar                 │  Terminal Header          │
│            ├──────────────────────────┤  (node name, status)     │
│  Sidebar   │                          ├──────────────────────────┤
│  (260px)   │                          │                          │
│            │       Canvas             │     Terminal Panel        │
│            │    (Decision Tree)       │     (PTY output)         │
│            │                          │                          │
│            │                          │                          │
│            │                          │                          │
├────────────┴──────────────────────────┴──────────────────────────┤
│  Status Bar                                                      │
└──────────────────────────────────────────────────────────────────┘
```

The terminal panel slides in from the right, taking 45% of the content area. The canvas compresses but remains visible and interactive. The divider between canvas and terminal is draggable.

Pressing `Escape` or clicking the canvas background closes the terminal panel and returns to full-canvas view.

---

## 4. Color System

### Base Palette

| Token | Hex | Usage |
|---|---|---|
| `bg-base` | `#0E1117` | Canvas background |
| `bg-surface` | `#161B22` | Sidebar, panels, modal cards |
| `bg-elevated` | `#1C2128` | Nodes, input fields, hover states |
| `bg-overlay` | `#2D333B` | Active node, dropdowns, tooltips |
| `border-default` | `#30363D` | Dividers, node borders, input borders |
| `border-subtle` | `#21262D` | Low-contrast separators, grid lines |

### Text

| Token | Hex | Usage |
|---|---|---|
| `text-primary` | `#E6EDF3` | Node labels, headings, primary content |
| `text-secondary` | `#8B949E` | Metadata, descriptions, timestamps |
| `text-muted` | `#484F58` | Placeholders, disabled states, grid dots |

### Semantic & Node States

| Token | Hex | Usage |
|---|---|---|
| `accent` | `#58A6FF` | Focus rings, active selection, primary actions |
| `node-running` | `#3FB950` | Running node border + glow, pulse animation |
| `node-completed` | `#8B949E` | Completed node border (neutral, settled) |
| `node-paused` | `#D29922` | Paused node border, waiting for user action |
| `node-failed` | `#F85149` | Failed node border, error states |
| `node-pending` | `#484F58` | Pending/queued node, dashed border |
| `edge-default` | `#30363D` | Tree edges (connecting lines) |
| `edge-active` | `#58A6FF` | Edge leading to selected node |
| `merge-highlight` | `#A371F7` | Merge candidate highlight, merge actions |

### Terminal

| Element | Value |
|---|---|
| Background | `#0E1117` |
| Foreground | `#E6EDF3` |
| Cursor | `#58A6FF` (block, blinking) |
| Selection | `#264F78` |

---

## 5. Typography

Monospace throughout. Same rationale as before: the app is a terminal-adjacent tool, and visual consistency between UI chrome, node labels, and terminal output reduces cognitive switching.

| Element | Size | Weight |
|---|---|---|
| Agent name (sidebar) | 13px | 600 |
| Node label | 12px | 500 |
| Node metadata (commit, prompt preview) | 11px | 400 |
| Toolbar text | 12px | 400 |
| Modal headings | 15px | 600 |
| Terminal content | 14px | 400 |
| Status bar | 11px | 400 |

Font stack: `JetBrains Mono`, `SF Mono`, `Cascadia Code`, `monospace`.

---

## 6. Component Details

### 6.1 Sidebar — Agent List

The sidebar lists all configured agents. Each agent maps to one git repo and one decision tree.

**Header:**
- App wordmark "crongen" in `text-secondary`, 11px, uppercase, letter-spaced.
- "+ New Agent" button: full-width, `bg-elevated`, `accent` left border (3px).

**Agent card:**

```
┌────────────────────────────────┐
│  ● Nightly Tests               │
│  ~/repos/myproject             │
│  zsh · 0 2 * * * · US/East    │
│  3 branches · 1 running       │
└────────────────────────────────┘
```

- **Status dot:** `node-running` green if any branch is active, `text-muted` otherwise.
- **Agent name:** `text-primary`, 13px, semi-bold.
- **Repo path:** `text-secondary`, truncated from the left (`…/myproject`).
- **Metadata line:** Shell badge, cron expression, timezone.
- **Branch summary:** "3 branches · 1 running" in `text-muted`. Gives a quick sense of tree size and activity.
- **Overflow menu** (`⋮`, visible on hover): Edit, Delete, Run Now.

**Selected agent:** `accent` left border (3px), `bg-elevated` background. The canvas shows this agent's decision tree.

### 6.2 Toolbar

Horizontal bar between the sidebar and the canvas. Context-sensitive to the selected agent.

```
┌─────────────────────────────────────────────────────────────┐
│  Nightly Tests  ·  ~/repos/myproject      ⊕ Fork   🔀 Merge │
│                                       [-] [○] [+]  Fit All  │
└─────────────────────────────────────────────────────────────┘
```

**Left section:**
- Agent name (`text-primary`, 13px, semi-bold).
- Repo path (`text-secondary`).

**Right section:**
- **Fork button:** Creates a new branch from the currently selected node. Disabled when no node is selected or the selected node is still running.
- **Merge button:** Opens the merge flow for the selected branch. Disabled unless a completed leaf node is selected.
- **Zoom controls:** `[-]` `[○]` `[+]` — zoom out, fit-to-view, zoom in. Small icon buttons in `text-secondary`.
- **Fit All:** Resets canvas viewport to show the entire tree.

### 6.3 Canvas — Decision Tree

The canvas is a pannable, zoomable surface rendered with SVG or Canvas2D (implementation detail). It displays the agent's decision tree as a top-down directed graph.

#### Tree Layout

The tree flows **top to bottom**. The root node is at the top. Branches grow downward. This matches the natural reading direction and the mental model of "decisions cascade down."

```
            ┌──────────┐
            │  Root     │  ← Initial run
            │  main     │
            └────┬─────┘
                 │
         ┌───────┼──────────┐
         │       │           │
    ┌────▼──┐ ┌──▼────┐ ┌───▼────┐
    │ Fix A │ │ Fix B │ │ Fix C  │  ← User forked 3 approaches
    │ ✓ 0   │ │ ● run │ │ ⏸ wait │
    └───────┘ └───┬───┘ └────────┘
                  │
             ┌────▼────┐
             │ Fix B.1 │  ← Further exploration on branch B
             │ ● run   │
             └─────────┘
```

#### Node Design

Each node is a rounded rectangle (8px radius), approximately 180px wide × variable height.

```
┌──────────────────────────┐
│  Fix approach A          │  ← Label (user-provided or auto)
│  ─────────────────────── │
│  abc1234 · 2m ago        │  ← Commit hash (short) + timestamp
│  "Try fixing with..."    │  ← Prompt preview (truncated, 1 line)
│                     ✓ 0  │  ← Status icon + exit code
└──────────────────────────┘
```

**Node states and visual treatment:**

| State | Border | Icon | Extra |
|---|---|---|---|
| `pending` | 1px dashed `node-pending` | `◌` hollow circle | Queued, waiting to start |
| `running` | 2px solid `node-running` | `●` filled, pulsing | Subtle green glow (box-shadow, 8px spread, 20% opacity), pulse animation on the status icon (1s interval) |
| `paused` | 2px solid `node-paused` | `⏸` pause icon | Amber border, waiting for user |
| `completed` | 1px solid `node-completed` | `✓` checkmark + exit code | Neutral, settled appearance |
| `failed` | 2px solid `node-failed` | `✗` cross + exit code | Red border |

**Background:** `bg-elevated` default. On hover: lighten slightly. On selection: `bg-overlay` with `accent` border replacing the state border.

**Selected node:** `accent` border (2px), all edges leading to this node highlight in `edge-active`. The terminal panel opens showing this node's session.

#### Edges

Edges are smooth cubic bezier curves connecting parent to child nodes. Drawn in `edge-default`. The edge leading from the root to the currently selected node's ancestry path highlights in `edge-active`.

Edge labels are not needed for v1.0. The prompt on each node provides sufficient context.

#### Canvas Interactions

- **Pan:** Click-drag on empty canvas space, or two-finger scroll.
- **Zoom:** Scroll wheel, pinch gesture, or toolbar buttons. Range: 25%–200%.
- **Select node:** Single click. Opens terminal panel if not already open.
- **Deselect:** Click empty canvas space or press `Escape`. Terminal panel closes.
- **Context menu:** Right-click a node for: Fork from here, View diff, Copy commit hash, Delete branch.
- **Double-click node:** Opens the node detail / edit prompt panel (see §6.5).

#### Empty State

When an agent has no runs yet:

```
          ┌─────────────────────────┐
          │     ▶ Start first run   │
          │                         │
          │  This agent has no      │
          │  decision history yet.  │
          │  Run it to create the   │
          │  root node.             │
          └─────────────────────────┘
```

Single centered call-to-action node, styled as a dashed-border ghost node. Clicking it triggers `run_task_now`.

### 6.4 Terminal Panel

Slides in from the right when a node is selected. Takes 45% of the content area width (draggable divider, min 30%, max 70%).

**Header:**

```
┌──────────────────────────────────┐
│  ● Fix approach B    abc1234  ×  │
│  worktree: ~/repos/myproject-b   │
├──────────────────────────────────┤
│                                  │
│  $ claude -p "Try approach B..." │
│  Analyzing codebase...           │
│  Found 3 files to modify...      │
│  ...                             │
│                                  │
└──────────────────────────────────┘
```

- **Status dot:** Matches node state color.
- **Node label + commit hash:** `text-primary` + `text-secondary`.
- **Close button** (`×`): Closes the panel, deselects the node.
- **Worktree path:** `text-muted`, 11px. Shows the filesystem path of this branch's worktree.

**Terminal area:** Full xterm.js instance, same config as the original spec (base64 decoding, fit addon, resize forwarding). Interactive when the session is running — user can type to steer the agent.

**Exited session:** Terminal content remains scrollable. A status bar appears at the top of the terminal area:
- Exit 0: `node-completed` background tint, "Completed successfully".
- Non-zero: `node-failed` background tint, "Exited with code N".

**Action buttons** (below terminal header, contextual):

| Session State | Available Actions |
|---|---|
| Running | `⏸ Pause` · `Fork` (disabled until paused/done) |
| Paused | `▶ Resume` · `⊕ Fork from here` |
| Completed | `⊕ Fork from here` · `🔀 Merge this branch` · `View Diff` |
| Failed | `⊕ Fork from here` (retry with different prompt) · `View Diff` |

### 6.5 Fork Flow

Forking is the core interaction. It creates a new branch in the decision tree.

**Trigger:** User clicks "Fork from here" on a paused or completed node.

**Fork Modal:**

```
┌──────────────────────────────────────┐
│  Fork from "Fix approach B"       ×  │
│  Commit: abc1234                     │
├──────────────────────────────────────┤
│                                      │
│  Branch Name                         │
│  ┌────────────────────────────────┐  │
│  │ fix-approach-b-alt             │  │
│  └────────────────────────────────┘  │
│                                      │
│  New Prompt                          │
│  ┌────────────────────────────────┐  │
│  │ Try a different approach:     ││  │
│  │ use the adapter pattern       ││  │
│  │ instead of inheritance...     ││  │
│  └────────────────────────────────┘  │
│                                      │
│  ☐ Start immediately                 │
│                                      │
│            ┌────────┐  ┌──────────┐  │
│            │ Cancel │  │  Fork    │  │
│            └────────┘  └──────────┘  │
└──────────────────────────────────────┘
```

- **Branch name:** Auto-generated from parent label + suffix. User can edit.
- **New prompt:** The instruction to inject into the new PTY session. Required.
- **Start immediately:** Checkbox, default checked. If unchecked, the node is created in `pending` state.
- On confirm: backend runs `git worktree add`, creates a new node in the tree, and (if start immediately) spawns a PTY session in the new worktree directory.
- The new node animates into the tree (fade in + slide down from parent), and auto-selects (opening the terminal panel).

### 6.6 Merge Flow

Merging takes a completed branch and integrates it back into the repo's main branch.

**Trigger:** User clicks "Merge this branch" on a completed leaf node.

**Merge Confirmation Panel** (inline in the terminal panel, replaces action buttons):

```
┌──────────────────────────────────┐
│  Merge "fix-approach-b" → main   │
│                                  │
│  This will:                      │
│  · Merge worktree changes into   │
│    the main branch               │
│  · Clean up the worktree from    │
│    disk                          │
│                                  │
│  ┌────────┐  ┌────────────────┐  │
│  │ Cancel │  │  Confirm Merge │  │
│  └────────┘  └────────────────┘  │
└──────────────────────────────────┘
```

- **Confirm Merge** button uses `merge-highlight` purple background.
- On success: the merged node gets a purple `merge-highlight` border and a merge icon (`🔀`). Sibling branches remain visible but are visually dimmed (30% opacity) to indicate they were not chosen.
- On conflict: display the conflict output in the terminal panel. The user resolves conflicts using the terminal (interactive PTY in the worktree), then confirms.

### 6.7 Task/Agent Modal

Same structure as the original TaskModal, with additions for the repo binding.

```
┌──────────────────────────────────────┐
│  New Agent                        ×  │
├──────────────────────────────────────┤
│                                      │
│  Name                                │
│  ┌────────────────────────────────┐  │
│  │ Nightly Test Suite             │  │
│  └────────────────────────────────┘  │
│                                      │
│  Repository Path                     │
│  ┌──────────────────────────┐        │
│  │ ~/repos/myproject        │ [📁]   │
│  └──────────────────────────┘        │
│  ✓ Valid git repository              │
│                                      │
│  Shell                               │
│  ┌────────────────────────────────┐  │
│  │ zsh                          ▼ │  │
│  └────────────────────────────────┘  │
│                                      │
│  Initial Prompt                      │
│  ┌────────────────────────────────┐  │
│  │ claude -p "Run the full test  ││  │
│  │ suite and report failures"    ││  │
│  └────────────────────────────────┘  │
│                                      │
│  Schedule           Timezone         │
│  ┌──────────────┐  ┌──────────────┐  │
│  │ 0 2 * * *    │  │ US/Eastern ▼ │  │
│  └──────────────┘  └──────────────┘  │
│  ✓ Every day at 2:00 AM              │
│                                      │
│            ┌────────┐  ┌──────────┐  │
│            │ Cancel │  │   Save   │  │
│            └────────┘  └──────────┘  │
└──────────────────────────────────────┘
```

- **Repository Path:** Text input with a folder-picker button (`📁`). On input, validate that the path is a valid git repository (check for `.git`). Show green checkmark or red error below.
- **Initial Prompt:** This becomes the root node's prompt when the agent first runs.
- All other fields same as the original spec.

### 6.8 Status Bar

```
┌──────────────────────────────────────────────────────────────────┐
│  ● Scheduler active   2 agents · 5 branches · 1 running   Next: 47m │
└──────────────────────────────────────────────────────────────────┘
```

- **Scheduler indicator:** Green/red dot.
- **Global summary:** Agent count, total branches across all agents, running session count.
- **Next trigger:** Next scheduled cron trigger with countdown.

### 6.9 Toast Notifications

Same behavior as the original spec. Additional toast types:

- **Fork created:** `accent` left border, "Forked from [node] → [new branch]".
- **Merge complete:** `merge-highlight` left border, "Merged [branch] into main".
- **Merge conflict:** `node-failed` left border, "Merge conflict — resolve in the terminal".
- **Worktree error:** `node-failed` left border, "Failed to create worktree: [reason]".

---

## 7. Interaction Patterns

### 7.1 First Run — Creating the Root Node
1. User creates an agent (modal), specifying the repo and initial prompt.
2. User clicks "Run Now" from the sidebar (or waits for cron trigger).
3. Backend runs `git worktree add` from the current HEAD of the main branch, spawns a PTY in that worktree.
4. The root node appears on the canvas, status `running`. Auto-selects, opening the terminal panel.

### 7.2 Exploring a Branch — Mid-Run Fork
1. Agent is running (green pulsing node). User is watching output in the terminal panel.
2. User sees the agent going down a wrong path. Clicks `⏸ Pause`.
3. Node status transitions to `paused` (amber). The PTY session receives `SIGTSTP` (or equivalent).
4. User clicks "Fork from here." Fork modal opens with the current commit.
5. User writes a new prompt ("Try the adapter pattern instead"). Clicks Fork.
6. A new child node appears below the paused node. New worktree is created from the same commit. New PTY session starts with the new prompt.
7. The original paused node can be resumed later, or left paused.

### 7.3 Exploring a Branch — Post-Completion Fork
1. Agent completes (node shows `✓ 0`).
2. User reviews the output. Not satisfied.
3. Clicks "Fork from here." Same flow as above, but the fork point is the completed commit.
4. User can create multiple forks from the same node, exploring several alternatives in parallel.

### 7.4 Choosing a Winner — Merge
1. User identifies the best branch (a completed leaf node).
2. Clicks "Merge this branch."
3. Confirmation panel appears. User confirms.
4. Backend runs `git merge` into main. On success, the node gets the merge indicator, sibling branches dim.
5. Worktrees for the merged branch (and optionally abandoned branches) are cleaned up.

### 7.5 Navigating the Tree
- Click a node to select it and open its terminal.
- Arrow keys navigate between nodes (↑ parent, ↓ first child, ← previous sibling, → next sibling).
- `Ctrl/Cmd + 0` fits the entire tree in view.
- Scroll to zoom. Drag empty space to pan.

---

## 8. Keyboard Shortcuts

| Action | Shortcut |
|---|---|
| New Agent | `Ctrl/Cmd + N` |
| Toggle Sidebar | `Ctrl/Cmd + B` |
| Close Terminal Panel | `Escape` |
| Focus Terminal | `Enter` (when node selected) |
| Focus Canvas | `Escape` (when terminal focused) |
| Navigate tree | `Arrow keys` (when canvas focused) |
| Fit tree to view | `Ctrl/Cmd + 0` |
| Zoom in/out | `Ctrl/Cmd + =` / `Ctrl/Cmd + -` |
| Fork selected node | `Ctrl/Cmd + F` |
| Run agent now | `Ctrl/Cmd + R` |
| Pause/Resume session | `Ctrl/Cmd + P` |

When the terminal panel is focused, all shortcuts except `Escape` and panel-level actions pass through to the PTY.

---

## 9. Responsive Behavior

**Minimum window size:** 900 × 600px.

**Narrow windows (<1100px):** Sidebar auto-collapses to icon-only (48px). Terminal panel opens as an overlay instead of a side split (slides up from the bottom, 50% height).

**Terminal panel:** Draggable divider between canvas and terminal. Min 30%, max 70% of content width. Remembers last position per session.

---

## 10. Motion & Animation

| Element | Animation | Duration | Easing |
|---|---|---|---|
| Terminal panel open | Slide in from right + fade | 200ms | ease-out |
| Terminal panel close | Slide out right + fade | 150ms | ease-in |
| New node appears | Fade in + slide down from parent edge | 250ms | ease-out |
| Node state change | Border color crossfade | 300ms | ease-in-out |
| Running node glow | Pulse (opacity 15%→30%→15%) | 2s | sinusoidal, looping |
| Running status dot | Pulse (scale 1→1.3→1) | 1.5s | sinusoidal, looping |
| Merged siblings dim | Opacity → 30% | 500ms | ease-out |
| Edge highlight | Color transition | 200ms | ease-out |
| Canvas pan/zoom | Momentum-based | 300ms | ease-out (decelerate) |
| Modal open/close | Fade + scale 98%→100% | 150ms | ease-out |
| Toast appear/dismiss | Slide from bottom-right / fade out | 200ms / 150ms | ease-out / ease-in |

Respect `prefers-reduced-motion`: disable glow/pulse animations, keep only opacity and color transitions.

---

## 11. Backend & Data Model Implications

This section flags changes to the revised spec (v1.0.0) required to support the canvas paradigm. Full specification belongs in the main spec document.

### New Data

**`decision_nodes` table:**

```sql
CREATE TABLE IF NOT EXISTS decision_nodes (
    id            TEXT PRIMARY KEY,          -- UUID v4
    agent_id      TEXT NOT NULL REFERENCES scheduled_tasks(id),
    parent_id     TEXT REFERENCES decision_nodes(id),  -- NULL for root
    label         TEXT NOT NULL,
    prompt        TEXT NOT NULL,
    branch_name   TEXT NOT NULL,             -- Git branch name
    worktree_path TEXT,                      -- Filesystem path to worktree
    commit_hash   TEXT,                      -- Git commit at this node's state
    status        TEXT NOT NULL DEFAULT 'pending',  -- pending|running|paused|completed|failed
    exit_code     INTEGER,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
```

### New IPC Commands

| Command | Description |
|---|---|
| `get_decision_tree(agent_id)` | Returns all nodes for an agent as a flat list (frontend builds the tree) |
| `fork_node(node_id, label, prompt, start_immediately)` | Creates worktree, inserts child node, optionally starts session |
| `pause_session(session_id)` | Sends SIGTSTP to the PTY child process |
| `resume_session(session_id)` | Sends SIGCONT to the PTY child process |
| `merge_branch(node_id)` | Merges the node's branch into main, cleans up worktree |
| `delete_branch(node_id)` | Removes worktree and node (plus descendants) |

### Modified Concepts

- `run_task_now` now creates the **root node** of a decision tree (if none exists) or returns an error if a root already exists and is running.
- `session_started` event payload adds `node_id` so the frontend can associate the session with a tree node.
- `PtyManager::spawn_session` now accepts a `worktree_path` as the working directory for the PTY.
- The `ScheduledTask` (now "Agent") gains a `repo_path: String` field.
