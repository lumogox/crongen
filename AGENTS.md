# crongen

crongen is a Tauri 2 desktop app for scheduling, orchestrating, and monitoring autonomous coding agents. Users configure agents against git repositories, run them as branching decision trees, and inspect execution through a visual canvas with integrated terminals.

## Stack

- Desktop shell: Tauri 2.x
- Frontend: React 19 + TypeScript + Vite + Tailwind CSS v4
- Canvas: `@xyflow/react` v12 + `@dagrejs/dagre`
- Terminal: `@xterm/xterm` v5
- Backend: Rust with `rusqlite`, `portable-pty`, `git2`, `tokio-cron-scheduler`, and `toon-format`
- Package manager: Bun

## Repo Map

- `src/` - React frontend
  - `App.tsx` - top-level state and event wiring
  - `components/` - UI, canvas, terminal, modal, and inspector components
  - `hooks/` - terminal, SDK, and tree layout hooks
  - `lib/tauri-commands.ts` - typed Tauri IPC wrappers
  - `index.css` - design tokens and global styling
  - `types/` - frontend domain and event types
- `src-tauri/src/` - Rust backend
  - `main.rs` - native entry point
  - `lib.rs` - Tauri setup and command registration
  - `commands.rs` - IPC handlers
  - `db.rs` - SQLite schema and CRUD
  - `git_manager.rs` - worktrees, branches, and merges
  - `pty_manager.rs` - PTY lifecycle
  - `sdk_manager.rs` - headless SDK execution
  - `orchestrator.rs` - auto/supervised execution loop
  - `plan_generator.rs` - structured plan generation
  - `context.rs` and `toon.rs` - execution context building
  - `agent_templates.rs` and `models.rs` - agent configuration and domain types
- Project docs:
  - `README.md` - product overview and workflows
  - `crongen-spec-v2.md` - product and system spec
  - `crongen-ui-design.md` - UI design spec
  - `crongen.pen` - design source

## Commands

- `bun install` - install frontend dependencies
- `bun run dev` - Vite dev server for the frontend only
- `bun run tauri dev` - full desktop app with Rust backend
- `bun run build` - frontend production build
- `bun run tauri build` - distributable desktop build
- `bunx tsc --noEmit` - TypeScript check
- `cargo check` - Rust check from `src-tauri/` when Cargo is available

## Conventions

- Use Bun for all JavaScript and TypeScript work; do not introduce npm, yarn, or pnpm workflows.
- Keep Rust IPC handlers returning `Result<T, String>`.
- Frontend-to-backend communication should go through `@tauri-apps/api` `invoke()`.
- Backend-to-frontend updates should use emitted events such as `session_started`, `session_ended`, `pty_output`, `sdk_output`, and orchestrator progress events.
- Keep the dark theme, JetBrains Mono, and design tokens consistent with `src/index.css` and `crongen.pen`.
- Preserve the git worktree model and branching terminology used by the app: task, decision, agent, merge, final.
- Avoid committing generated artifacts or OS junk; the tracked lockfile is `bun.lock`.
