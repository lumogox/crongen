use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::models::{AutoResponse, SessionCompletion};

use anyhow::{Context, Result};
use base64::prelude::*;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

use crate::db;
use crate::models::NodeStatus;

// ─── Event Payloads ──────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
pub struct SessionStartedPayload {
    pub session_id: String,
    pub node_id: String,
    pub project_id: String,
}

#[derive(Clone, serde::Serialize)]
pub struct PtyOutputPayload {
    pub session_id: String,
    pub data: String, // base64 encoded
}

#[derive(Clone, serde::Serialize)]
pub struct SessionEndedPayload {
    pub session_id: String,
    pub node_id: String,
    pub exit_code: Option<i32>,
}

// ─── Active Session ──────────────────────────────────────────

struct ActiveSession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    #[allow(dead_code)]
    process_id: Option<u32>,
    project_id: String,
    #[allow(dead_code)]
    node_id: String,
}

// ─── PTY Manager ─────────────────────────────────────────────

// Max output buffer per session: 1 MB
const OUTPUT_BUFFER_CAP: usize = 1_048_576;

pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, ActiveSession>>>,
    output_buffers: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    logs_dir: PathBuf,
    completion_tx: broadcast::Sender<SessionCompletion>,
}

impl PtyManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let logs_dir = app_data_dir.join("session_logs");
        std::fs::create_dir_all(&logs_dir).expect("Failed to create session_logs directory");
        let (completion_tx, _) = broadcast::channel(64);
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            output_buffers: Arc::new(Mutex::new(HashMap::new())),
            logs_dir,
            completion_tx,
        }
    }

    /// Subscribe to session completion events (used by orchestrator).
    pub fn subscribe_completions(&self) -> broadcast::Receiver<SessionCompletion> {
        self.completion_tx.subscribe()
    }

    /// Spawn a new PTY session for a project-scoped node run.
    ///
    /// Uses the node ID as session ID (1:1 mapping between nodes and PTY sessions).
    /// The process runs in the specified working directory (worktree path).
    /// Output is base64-encoded and emitted as `pty_output` Tauri events.
    /// On process exit, node status is updated in DB and `session_ended` is emitted.
    pub fn spawn_session(
        &self,
        session_id: &str,
        project_id: &str,
        node_id: &str,
        program: &str,
        args: &[String],
        cwd: &str,
        stdin_injection: Option<&str>,
        auto_responses: Vec<AutoResponse>,
        db: Arc<Mutex<rusqlite::Connection>>,
        app: AppHandle,
    ) -> Result<()> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);

        // Inherit full environment (PATH, HOME, API keys, NVM paths, etc.)
        // portable-pty does NOT inherit parent env by default.
        for (key, val) in std::env::vars() {
            cmd.env(key, val);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn command in PTY")?;

        let process_id = child.process_id();

        // Must drop slave after spawn — child now owns it
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;

        // take_writer() can only be called once per MasterPty.
        // Dropping it sends EOF to slave, so we keep it in the session.
        let mut writer = pair
            .master
            .take_writer()
            .context("Failed to take PTY writer")?;

        // Stdin injection: wait for TUI/shell to initialize, write the prompt
        // text, pause for the input handler to process it, then send Enter (\r)
        // as a separate write. Splitting text from submit is critical for TUI
        // terminal-backed agents (Gemini, custom shells) — their Ink/React input handlers
        // may treat a single write containing text+\r as a paste event rather
        // than recognizing \r as a distinct Enter keypress.
        if let Some(injection) = stdin_injection {
            log::info!("Stdin injection for session {}: waiting 1500ms", session_id);
            thread::sleep(std::time::Duration::from_millis(1500));
            if let Err(e) = writer.write_all(injection.as_bytes()) {
                log::error!(
                    "Stdin injection write failed for session {}: {}",
                    session_id,
                    e
                );
            }
            if let Err(e) = writer.flush() {
                log::error!(
                    "Stdin injection flush failed for session {}: {}",
                    session_id,
                    e
                );
            }
            thread::sleep(std::time::Duration::from_millis(200));
            if let Err(e) = writer.write_all(b"\r") {
                log::error!(
                    "Stdin injection submit failed for session {}: {}",
                    session_id,
                    e
                );
            }
            if let Err(e) = writer.flush() {
                log::error!(
                    "Stdin injection submit flush failed for session {}: {}",
                    session_id,
                    e
                );
            }
            log::info!("Stdin injection completed for session {}", session_id);
        }

        // Store session in map + initialize output buffer
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(
                session_id.to_string(),
                ActiveSession {
                    writer,
                    master: pair.master,
                    process_id,
                    project_id: project_id.to_string(),
                    node_id: node_id.to_string(),
                },
            );
        }
        {
            let mut buffers = self.output_buffers.lock().unwrap();
            buffers.insert(session_id.to_string(), Vec::new());
        }

        // Update node status to Running
        if let Ok(conn) = db.lock() {
            let _ = db::node_update_status(&conn, node_id, &NodeStatus::Running, None);
        }

        // Emit session_started event
        let _ = app.emit(
            "session_started",
            SessionStartedPayload {
                session_id: session_id.to_string(),
                node_id: node_id.to_string(),
                project_id: project_id.to_string(),
            },
        );

        // Spawn reader thread: reads PTY output → buffer + base64 + log file → Tauri event
        // Also handles auto-responses: pattern-matches output and injects stdin responses.
        let sid = session_id.to_string();
        let nid = node_id.to_string();
        let worktree_path = cwd.to_string();
        let sessions_cleanup = self.sessions.clone();
        let sessions_auto = self.sessions.clone();
        let output_buffers = self.output_buffers.clone();
        let app_reader = app.clone();
        let db_reader = db.clone();
        let log_path = self.logs_dir.join(format!("{}.log", session_id));
        let completion_tx = self.completion_tx.clone();

        thread::spawn(move || {
            let mut buf = vec![0u8; 16384]; // 16KB buffer
            let mut matched_patterns: HashSet<usize> = HashSet::new();

            // Open log file for this session (append mode in case of restart)
            let mut log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok();

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF — process exited
                    Ok(n) => {
                        let chunk = &buf[..n];

                        // Auto-respond to known interactive prompts.
                        // Each pattern fires at most once per session.
                        if !auto_responses.is_empty() {
                            let chunk_str = String::from_utf8_lossy(chunk);
                            let chunk_lower = chunk_str.to_lowercase();
                            for (i, ar) in auto_responses.iter().enumerate() {
                                let pattern = ar.pattern.to_lowercase();
                                if !matched_patterns.contains(&i) && chunk_lower.contains(&pattern)
                                {
                                    matched_patterns.insert(i);
                                    log::info!(
                                        "Auto-responding to '{}' in session {}",
                                        ar.pattern,
                                        sid
                                    );
                                    thread::sleep(std::time::Duration::from_millis(ar.delay_ms));
                                    if let Ok(mut sessions) = sessions_auto.lock() {
                                        if let Some(session) = sessions.get_mut(&sid) {
                                            if !ar.response.is_empty() {
                                                let _ = session
                                                    .writer
                                                    .write_all(ar.response.as_bytes());
                                                let _ = session.writer.flush();
                                            }
                                            if ar.submit {
                                                thread::sleep(std::time::Duration::from_millis(
                                                    200,
                                                ));
                                                let _ = session.writer.write_all(b"\r");
                                                let _ = session.writer.flush();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Tee to output buffer (capped at 1MB)
                        if let Ok(mut buffers) = output_buffers.lock() {
                            if let Some(ob) = buffers.get_mut(&sid) {
                                if ob.len() < OUTPUT_BUFFER_CAP {
                                    let remaining = OUTPUT_BUFFER_CAP - ob.len();
                                    ob.extend_from_slice(&chunk[..n.min(remaining)]);
                                }
                            }
                        }

                        // Tee to log file on disk
                        if let Some(ref mut f) = log_file {
                            let _ = f.write_all(chunk);
                        }

                        let encoded = BASE64_STANDARD.encode(chunk);
                        let _ = app_reader.emit(
                            "pty_output",
                            PtyOutputPayload {
                                session_id: sid.clone(),
                                data: encoded,
                            },
                        );
                    }
                    Err(e) => {
                        log::warn!("PTY read error for session {}: {}", sid, e);
                        break;
                    }
                }
            }

            // Flush log file before exit
            if let Some(ref mut f) = log_file {
                let _ = f.flush();
            }

            // Wait for child process to exit
            let exit_code = match child.wait() {
                Ok(status) => {
                    if status.success() {
                        Some(0)
                    } else {
                        // portable-pty ExitStatus doesn't expose exact code on all platforms
                        Some(1)
                    }
                }
                Err(_) => None,
            };

            // Auto-commit any uncommitted agent work, then capture the final hash.
            // Agents don't always commit their changes before exiting.
            match crate::git_manager::auto_commit_worktree(&worktree_path) {
                Ok(committed) => {
                    if committed {
                        log::info!("Auto-committed uncommitted work for node {}", nid);
                    }
                }
                Err(e) => {
                    log::warn!("Auto-commit failed for node {}: {e}", nid);
                }
            }
            match crate::git_manager::get_current_commit(&worktree_path) {
                Ok(final_hash) => {
                    if let Ok(conn) = db_reader.lock() {
                        let _ = db::node_update_commit(&conn, &nid, &final_hash);
                    }
                }
                Err(e) => {
                    log::warn!("Failed to get final commit for node {}: {e}", nid);
                }
            }

            // Update node status in DB
            let status = match exit_code {
                Some(0) => NodeStatus::Completed,
                _ => NodeStatus::Failed,
            };

            if let Ok(conn) = db_reader.lock() {
                let _ = db::node_update_status(&conn, &nid, &status, exit_code);
            }

            // Emit session_ended event
            let _ = app_reader.emit(
                "session_ended",
                SessionEndedPayload {
                    session_id: sid.clone(),
                    node_id: nid.clone(),
                    exit_code,
                },
            );

            // Notify orchestrator via broadcast channel
            let _ = completion_tx.send(SessionCompletion {
                node_id: nid,
                exit_code,
            });

            // Remove session from map (frees writer + master handles)
            {
                let mut sessions = sessions_cleanup.lock().unwrap();
                sessions.remove(&sid);
            }

            log::info!("Session {} ended with exit code {:?}", sid, exit_code);
        });

        log::info!(
            "Spawned PTY session {} for {} (pid={:?})",
            session_id,
            program,
            process_id,
        );

        Ok(())
    }

    /// Write data to a PTY session (forwards user keystrokes from xterm.js).
    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        session.writer.write_all(data)?;
        session.writer.flush()?;
        Ok(())
    }

    /// Resize a PTY session (xterm.js sends new dimensions on resize).
    pub fn resize(&self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        session.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Pause a PTY session by sending SIGSTOP to the process.
    #[cfg(unix)]
    pub fn pause_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;

        // Send SIGSTOP to the process group (negative pid)
        let ret = unsafe { libc::kill(-(pid as i32), libc::SIGSTOP) };
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "Failed to send SIGSTOP to process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        log::info!("Paused session {} (pid {})", session_id, pid);
        Ok(())
    }

    #[cfg(windows)]
    pub fn pause_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;
        drop(sessions);

        let suspended_threads = crate::windows_process::suspend_process(pid)?;
        log::info!(
            "Paused session {} (pid {}, suspended {} thread(s))",
            session_id,
            pid,
            suspended_threads
        );
        Ok(())
    }

    /// Resume a paused PTY session by sending SIGCONT to the process.
    #[cfg(unix)]
    pub fn resume_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;

        // Send SIGCONT to the process group (negative pid)
        let ret = unsafe { libc::kill(-(pid as i32), libc::SIGCONT) };
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "Failed to send SIGCONT to process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        log::info!("Resumed session {} (pid {})", session_id, pid);
        Ok(())
    }

    #[cfg(windows)]
    pub fn resume_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;
        drop(sessions);

        let resumed_threads = crate::windows_process::resume_process(pid)?;
        log::info!(
            "Resumed session {} (pid {}, resumed {} thread(s))",
            session_id,
            pid,
            resumed_threads
        );
        Ok(())
    }

    /// Stop a PTY session by terminating its process tree.
    #[cfg(unix)]
    pub fn stop_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;

        let ret = unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "Failed to stop process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        log::info!("Stopped session {} (pid {})", session_id, pid);
        Ok(())
    }

    #[cfg(windows)]
    pub fn stop_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let pid = session
            .process_id
            .ok_or_else(|| anyhow::anyhow!("No process ID for session: {session_id}"))?;
        drop(sessions);

        let output = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to stop session {}: {}",
                session_id,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        log::info!("Stopped session {} (pid {})", session_id, pid);
        Ok(())
    }

    /// Get buffered output for a session (base64-encoded).
    /// Checks in-memory buffer first (active sessions), then falls back
    /// to reading the log file from disk (completed/past sessions).
    pub fn get_buffered_output(&self, session_id: &str) -> Option<String> {
        // Try in-memory buffer first (active sessions)
        {
            let buffers = self.output_buffers.lock().unwrap();
            if let Some(b) = buffers.get(session_id) {
                if !b.is_empty() {
                    return Some(BASE64_STANDARD.encode(b));
                }
            }
        }

        // Fall back to log file on disk (completed sessions)
        let log_path = self.logs_dir.join(format!("{}.log", session_id));
        std::fs::read(&log_path)
            .ok()
            .filter(|b| !b.is_empty())
            .map(|b| BASE64_STANDARD.encode(&b))
    }

    /// Check if a project has any active PTY sessions.
    pub fn has_active_for_project(&self, project_id: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions.values().any(|s| s.project_id == project_id)
    }

    /// Check if a specific PTY session is currently active.
    pub fn has_session(&self, session_id: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions.contains_key(session_id)
    }

    /// Remove buffered output and persisted logs for a session.
    pub fn clear_session_artifacts(&self, session_id: &str) {
        {
            let mut buffers = self.output_buffers.lock().unwrap();
            buffers.remove(session_id);
        }

        let log_path = self.logs_dir.join(format!("{}.log", session_id));
        let _ = std::fs::remove_file(log_path);
    }

    /// Publish a synthetic completion event to unblock orchestrator recovery flows.
    pub fn publish_completion(&self, node_id: &str, exit_code: Option<i32>) {
        let _ = self.completion_tx.send(SessionCompletion {
            node_id: node_id.to_string(),
            exit_code,
        });
    }
}
