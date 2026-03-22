use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::db;
use crate::models::{NodeStatus, SdkOutputPayload, SessionCompletion};
use tokio::sync::broadcast;

// ─── Event Payloads ──────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
pub struct SdkSessionStartedPayload {
    pub session_id: String,
    pub node_id: String,
    pub project_id: String,
}

#[derive(Clone, serde::Serialize)]
pub struct SdkSessionEndedPayload {
    pub session_id: String,
    pub node_id: String,
    pub exit_code: Option<i32>,
}

// ─── Active Session ──────────────────────────────────────────

struct ActiveSession {
    process_id: u32,
    project_id: String,
    #[allow(dead_code)]
    node_id: String,
}

// ─── SDK Manager ─────────────────────────────────────────────

pub struct SdkManager {
    sessions: Arc<Mutex<HashMap<String, ActiveSession>>>,
    output_buffers: Arc<Mutex<HashMap<String, Vec<String>>>>,
    logs_dir: PathBuf,
    completion_tx: broadcast::Sender<SessionCompletion>,
}

impl SdkManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let logs_dir = app_data_dir.join("sdk_logs");
        std::fs::create_dir_all(&logs_dir).expect("Failed to create sdk_logs directory");
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

    /// Spawn a headless SDK session using `tokio::process::Command`.
    ///
    /// The child process writes structured JSON lines to stdout.
    /// Each line is stored in an in-memory buffer, appended to a `.jsonl` log,
    /// and emitted as an `sdk_output` Tauri event.
    pub fn spawn_session(
        &self,
        session_id: &str,
        project_id: &str,
        node_id: &str,
        program: &str,
        args: &[String],
        cwd: &str,
        db: Arc<Mutex<rusqlite::Connection>>,
        app: AppHandle,
    ) -> anyhow::Result<()> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        // Don't create a PTY — we want clean pipe output
        cmd.stdin(std::process::Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn SDK process: {e}"))?;

        let pid = child.id().unwrap_or(0);

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        // Initialize output buffer
        {
            let mut buffers = self.output_buffers.lock().unwrap();
            buffers.insert(session_id.to_string(), Vec::new());
        }

        // Store active session
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(
                session_id.to_string(),
                ActiveSession {
                    process_id: pid,
                    project_id: project_id.to_string(),
                    node_id: node_id.to_string(),
                },
            );
        }

        // Update node status to Running
        if let Ok(conn) = db.lock() {
            let _ = db::node_update_status(&conn, node_id, &NodeStatus::Running, None);
        }

        // Emit session_started event
        let _ = app.emit(
            "session_started",
            SdkSessionStartedPayload {
                session_id: session_id.to_string(),
                node_id: node_id.to_string(),
                project_id: project_id.to_string(),
            },
        );

        // Spawn async reader task for stdout (JSON lines)
        let sid = session_id.to_string();
        let nid = node_id.to_string();
        let worktree_path = cwd.to_string();
        let sessions_cleanup = self.sessions.clone();
        let output_buffers = self.output_buffers.clone();
        let app_reader = app.clone();
        let db_reader = db.clone();
        let log_path = self.logs_dir.join(format!("{}.jsonl", session_id));
        let completion_tx = self.completion_tx.clone();

        tokio::spawn(async move {
            // Open log file
            let mut log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok();

            // Read stdout line by line (each line is a JSON object)
            let mut stdout_reader = BufReader::new(stdout).lines();

            // Also drain stderr in background so it doesn't block
            let sid_stderr = sid.clone();
            tokio::spawn(async move {
                let mut stderr_reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = stderr_reader.next_line().await {
                    log::warn!("SDK stderr [{}]: {}", sid_stderr, line);
                }
            });

            while let Ok(Some(line)) = stdout_reader.next_line().await {
                // Store in memory buffer
                if let Ok(mut buffers) = output_buffers.lock() {
                    if let Some(buf) = buffers.get_mut(&sid) {
                        buf.push(line.clone());
                    }
                }

                // Append to log file
                if let Some(ref mut f) = log_file {
                    use std::io::Write;
                    let _ = writeln!(f, "{}", line);
                }

                // Emit Tauri event
                let _ = app_reader.emit(
                    "sdk_output",
                    SdkOutputPayload {
                        session_id: sid.clone(),
                        data: line,
                    },
                );
            }

            // Flush log file
            if let Some(ref mut f) = log_file {
                use std::io::Write;
                let _ = f.flush();
            }

            // Wait for child to exit
            let exit_code = match child.wait().await {
                Ok(status) => status.code(),
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
                        let _ = crate::db::node_update_commit(&conn, &nid, &final_hash);
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

            // Emit session_ended
            let _ = app_reader.emit(
                "session_ended",
                SdkSessionEndedPayload {
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

            // Remove from active sessions
            {
                let mut sessions = sessions_cleanup.lock().unwrap();
                sessions.remove(&sid);
            }

            log::info!("SDK session {} ended with exit code {:?}", sid, exit_code);
        });

        log::info!(
            "Spawned SDK session {} for {} (pid={})",
            session_id,
            program,
            pid,
        );

        Ok(())
    }

    /// Pause an SDK session by sending SIGSTOP.
    #[cfg(unix)]
    pub fn pause_session(&self, session_id: &str) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("SDK session not found: {session_id}"))?;

        let pid = session.process_id;
        let ret = unsafe { libc::kill(-(pid as i32), libc::SIGSTOP) };
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "Failed to send SIGSTOP to SDK process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        log::info!("Paused SDK session {} (pid {})", session_id, pid);
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn pause_session(&self, session_id: &str) -> anyhow::Result<()> {
        if !self.has_session(session_id) {
            return Err(anyhow::anyhow!("SDK session not found: {session_id}"));
        }

        Err(anyhow::anyhow!(
            "Pausing SDK sessions is not supported on this platform"
        ))
    }

    /// Resume a paused SDK session by sending SIGCONT.
    #[cfg(unix)]
    pub fn resume_session(&self, session_id: &str) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("SDK session not found: {session_id}"))?;

        let pid = session.process_id;
        let ret = unsafe { libc::kill(-(pid as i32), libc::SIGCONT) };
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "Failed to send SIGCONT to SDK process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        log::info!("Resumed SDK session {} (pid {})", session_id, pid);
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn resume_session(&self, session_id: &str) -> anyhow::Result<()> {
        if !self.has_session(session_id) {
            return Err(anyhow::anyhow!("SDK session not found: {session_id}"));
        }

        Err(anyhow::anyhow!(
            "Resuming SDK sessions is not supported on this platform"
        ))
    }

    /// Get buffered JSON lines for a session.
    /// Checks in-memory buffer first, then falls back to the `.jsonl` log file.
    pub fn get_buffered_output(&self, session_id: &str) -> Option<Vec<String>> {
        // Try in-memory buffer first
        {
            let buffers = self.output_buffers.lock().unwrap();
            if let Some(buf) = buffers.get(session_id) {
                if !buf.is_empty() {
                    return Some(buf.clone());
                }
            }
        }

        // Fall back to log file on disk
        let log_path = self.logs_dir.join(format!("{}.jsonl", session_id));
        std::fs::read_to_string(&log_path)
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.lines().map(|l| l.to_string()).collect())
    }

    /// Check if a project has any active SDK sessions.
    pub fn has_active_for_project(&self, project_id: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions.values().any(|s| s.project_id == project_id)
    }

    /// Check if a specific SDK session is currently active.
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

        let log_path = self.logs_dir.join(format!("{}.jsonl", session_id));
        let _ = std::fs::remove_file(log_path);
    }
}
