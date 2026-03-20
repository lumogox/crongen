use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, oneshot, Mutex as TokioMutex};

use crate::agent_templates;
use crate::context;
use crate::db;
use crate::git_manager;
use crate::models::*;
use crate::pty_manager::PtyManager;
use crate::sdk_manager::SdkManager;
use crate::toon;

// ─── Handle for a running orchestration ─────────────────────────

struct OrchestratorHandle {
    status: Arc<TokioMutex<OrchestratorStatus>>,
    /// Send a selected node ID to unblock a supervised decision.
    decision_tx: Option<oneshot::Sender<String>>,
    /// Cancel the orchestration loop.
    cancel_tx: Option<oneshot::Sender<()>>,
}

// ─── Orchestrator Manager ───────────────────────────────────────

pub struct OrchestratorManager {
    active: Arc<TokioMutex<HashMap<String, OrchestratorHandle>>>,
}

impl OrchestratorManager {
    pub fn new() -> Self {
        Self {
            active: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub async fn start_session(
        &self,
        session_root_id: String,
        mode: OrchestratorMode,
        db: Arc<Mutex<Connection>>,
        pty: Arc<PtyManager>,
        sdk: Arc<SdkManager>,
        app: AppHandle,
    ) -> Result<(), String> {
        let mut active = self.active.lock().await;
        if active.contains_key(&session_root_id) {
            return Err("Orchestrator already running for this session".to_string());
        }

        // Load subtree
        let root_id = session_root_id.clone();
        let db_clone = db.clone();
        let subtree = tokio::task::spawn_blocking(move || {
            let conn = db_clone.lock().map_err(|e| format!("DB lock: {e}"))?;
            db::node_get_subtree(&conn, &root_id).map_err(|e| format!("DB: {e}"))
        })
        .await
        .map_err(|e| format!("Task: {e}"))??;

        // Count all runnable nodes (task, agent, merge, final — everything except decision)
        let total_count = subtree
            .iter()
            .filter(|n| {
                let nt = n.node_type.as_deref().unwrap_or("agent");
                nt != "decision"
            })
            .count();

        // Count already-completed nodes (for resuming a partially-done session)
        let already_completed = subtree
            .iter()
            .filter(|n| {
                let nt = n.node_type.as_deref().unwrap_or("agent");
                nt != "decision"
                    && (n.status == NodeStatus::Completed || n.status == NodeStatus::Failed)
            })
            .count();

        // Reject if all runnable nodes are already done
        if already_completed >= total_count {
            return Err("All nodes in this session are already completed".to_string());
        }

        let status = Arc::new(TokioMutex::new(OrchestratorStatus {
            session_id: session_root_id.clone(),
            state: OrchestratorState::Running,
            mode: mode.clone(),
            current_node_id: None,
            completed_count: already_completed,
            total_count,
            pending_decision: None,
        }));

        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

        let handle = OrchestratorHandle {
            status: status.clone(),
            decision_tx: None,
            cancel_tx: Some(cancel_tx),
        };

        active.insert(session_root_id.clone(), handle);

        // Persist orchestrator state
        {
            let db_c = db.clone();
            let rid = session_root_id.clone();
            let m = mode.as_str().to_string();
            let _ = tokio::task::spawn_blocking(move || {
                let conn = db_c.lock().ok()?;
                db::orchestrator_upsert(&conn, &rid, &m, "running", None).ok()
            })
            .await;
        }

        // Subscribe to completions from both managers
        let mut pty_rx = pty.subscribe_completions();
        let mut sdk_rx = sdk.subscribe_completions();

        // Spawn the orchestration loop
        let active_ref = self.active.clone();
        let session_id = session_root_id.clone();

        tokio::spawn(async move {
            let result = orchestration_loop(
                &session_id,
                &mode,
                subtree,
                status.clone(),
                &active_ref,
                &mut pty_rx,
                &mut sdk_rx,
                cancel_rx,
                db.clone(),
                pty,
                sdk,
                app.clone(),
            )
            .await;

            let success = result.is_ok();
            let final_state = if success {
                OrchestratorState::Complete
            } else {
                OrchestratorState::Failed
            };

            // Update status
            {
                let mut s = status.lock().await;
                s.state = final_state.clone();
            }

            // Persist final state
            {
                let db_c = db.clone();
                let rid = session_id.clone();
                let state_str = final_state.as_str().to_string();
                let _ = tokio::task::spawn_blocking(move || {
                    let conn = db_c.lock().ok()?;
                    db::orchestrator_update_state(&conn, &rid, &state_str, None).ok()
                })
                .await;
            }

            // Emit completion event
            let _ = app.emit(
                "orchestrator_complete",
                OrchestratorCompletePayload {
                    session_id: session_id.clone(),
                    success,
                },
            );

            if let Err(ref e) = result {
                log::error!("Orchestrator failed for {}: {}", session_id, e);
            }

            // Clean up handle
            let mut active = active_ref.lock().await;
            active.remove(&session_id);
        });

        Ok(())
    }

    pub async fn get_status(&self, session_root_id: &str) -> Option<OrchestratorStatus> {
        let active = self.active.lock().await;
        if let Some(handle) = active.get(session_root_id) {
            Some(handle.status.lock().await.clone())
        } else {
            None
        }
    }

    pub async fn submit_decision(
        &self,
        session_root_id: &str,
        selected_node_id: String,
    ) -> Result<(), String> {
        let mut active = self.active.lock().await;
        if let Some(handle) = active.get_mut(session_root_id) {
            if let Some(tx) = handle.decision_tx.take() {
                tx.send(selected_node_id)
                    .map_err(|_| "Decision channel closed".to_string())?;
                Ok(())
            } else {
                Err("No pending decision for this session".to_string())
            }
        } else {
            Err("No active orchestrator for this session".to_string())
        }
    }

    pub async fn cancel_session(&self, session_root_id: &str) -> Result<(), String> {
        let mut active = self.active.lock().await;
        if let Some(handle) = active.get_mut(session_root_id) {
            if let Some(tx) = handle.cancel_tx.take() {
                let _ = tx.send(());
            }
            let mut s = handle.status.lock().await;
            s.state = OrchestratorState::Failed;
            Ok(())
        } else {
            Err("No active orchestrator for this session".to_string())
        }
    }
}

// ─── Orchestration Loop ─────────────────────────────────────────

async fn orchestration_loop(
    session_id: &str,
    mode: &OrchestratorMode,
    subtree: Vec<DecisionNode>,
    status: Arc<TokioMutex<OrchestratorStatus>>,
    active_ref: &Arc<TokioMutex<HashMap<String, OrchestratorHandle>>>,
    pty_rx: &mut broadcast::Receiver<SessionCompletion>,
    sdk_rx: &mut broadcast::Receiver<SessionCompletion>,
    mut cancel_rx: oneshot::Receiver<()>,
    db: Arc<Mutex<Connection>>,
    pty: Arc<PtyManager>,
    sdk: Arc<SdkManager>,
    app: AppHandle,
) -> Result<(), String> {
    // Build adjacency list: parent_id → children
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let node_map: HashMap<String, DecisionNode> =
        subtree.iter().map(|n| (n.id.clone(), n.clone())).collect();

    for node in &subtree {
        if let Some(ref pid) = node.parent_id {
            children_map
                .entry(pid.clone())
                .or_default()
                .push(node.id.clone());
        }
    }

    // Find the root (node with no parent in subtree)
    let root_id = subtree
        .first()
        .ok_or_else(|| "Empty subtree".to_string())?
        .id
        .clone();

    // Linearize the tree into execution order.
    // For decision nodes: run all agent children first, THEN merge/final children.
    // This ensures merge nodes execute only after all sibling agents complete.
    fn linearize(
        node_id: &str,
        node_map: &HashMap<String, DecisionNode>,
        children_map: &HashMap<String, Vec<String>>,
        out: &mut Vec<String>,
    ) {
        out.push(node_id.to_string());

        let Some(kids) = children_map.get(node_id) else {
            return;
        };

        let node = node_map.get(node_id);
        let node_type = node.and_then(|n| n.node_type.as_deref()).unwrap_or("agent");

        if node_type == "decision" {
            // Separate into agents and structural (merge/final)
            let mut agent_kids = Vec::new();
            let mut structural_kids = Vec::new();
            for kid_id in kids {
                if let Some(kid) = node_map.get(kid_id) {
                    let kt = kid.node_type.as_deref().unwrap_or("agent");
                    if kt == "merge" || kt == "final" {
                        structural_kids.push(kid_id.clone());
                    } else {
                        agent_kids.push(kid_id.clone());
                    }
                }
            }
            // Agents first, then structural
            for kid_id in &agent_kids {
                linearize(kid_id, node_map, children_map, out);
            }
            for kid_id in &structural_kids {
                linearize(kid_id, node_map, children_map, out);
            }
        } else {
            // For task/agent/merge/final: process children in order
            for kid_id in kids {
                linearize(kid_id, node_map, children_map, out);
            }
        }
    }

    let mut execution_order = Vec::new();
    linearize(&root_id, &node_map, &children_map, &mut execution_order);

    for node_id in &execution_order {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            return Err("Orchestrator cancelled".to_string());
        }

        let node = node_map
            .get(node_id)
            .ok_or_else(|| format!("Node {} not found in subtree", node_id))?;

        let node_type = node.node_type.as_deref().unwrap_or("agent");

        // Update status
        {
            let mut s = status.lock().await;
            s.current_node_id = Some(node_id.clone());
            s.state = OrchestratorState::Running;
            s.pending_decision = None;
        }

        // Persist current state
        {
            let db_c = db.clone();
            let sid = session_id.to_string();
            let nid = node_id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let conn = db_c.lock().ok()?;
                db::orchestrator_update_state(&conn, &sid, "running", Some(&nid)).ok()
            })
            .await;
        }

        match node_type {
            "task" | "agent" => {
                // Only run if the node is pending (skip already completed/failed)
                if node.status == NodeStatus::Pending {
                    run_single_node(node_id, &db, &pty, &sdk, &app).await?;

                    // Wait for completion
                    wait_for_completion(node_id, pty_rx, sdk_rx, &mut cancel_rx).await?;

                    // Update completed count (only for newly completed nodes)
                    {
                        let mut s = status.lock().await;
                        s.completed_count += 1;
                    }

                    // Emit progress
                    let s = status.lock().await;
                    let _ = app.emit(
                        "orchestrator_progress",
                        OrchestratorProgressPayload {
                            session_id: session_id.to_string(),
                            node_id: node_id.clone(),
                            status: "completed".to_string(),
                            completed_count: s.completed_count,
                            total_count: s.total_count,
                        },
                    );
                    drop(s);
                }
            }

            "decision" => {
                match mode {
                    OrchestratorMode::Auto => {
                        // Auto mode: all children already linearized in correct order — just continue
                    }
                    OrchestratorMode::Supervised => {
                        let kids = children_map.get(node_id).cloned().unwrap_or_default();

                        // Check if all agent children are already done (resuming a completed session).
                        // Re-fetch statuses from DB since node_map is a snapshot from session start.
                        let all_agents_done = {
                            let db_c = db.clone();
                            let kid_ids: Vec<String> = kids.clone();
                            let nm = node_map.clone();
                            tokio::task::spawn_blocking(move || {
                                let conn = db_c.lock().map_err(|e| format!("DB lock: {e}"))?;
                                for kid_id in &kid_ids {
                                    if let Some(kid) = nm.get(kid_id) {
                                        let kt = kid.node_type.as_deref().unwrap_or("agent");
                                        if kt == "merge" || kt == "final" {
                                            continue;
                                        }
                                        // Re-fetch current status from DB
                                        if let Ok(fresh) = db::node_get_by_id(&conn, kid_id) {
                                            if fresh.status == NodeStatus::Pending
                                                || fresh.status == NodeStatus::Running
                                                || fresh.status == NodeStatus::Paused
                                            {
                                                return Ok(false);
                                            }
                                        }
                                    }
                                }
                                Ok::<bool, String>(true)
                            })
                            .await
                            .map_err(|e| format!("Task: {e}"))??
                        };

                        if all_agents_done {
                            // All agent children already completed — skip the decision prompt
                            log::info!(
                                "Decision '{}' — all agent children done, skipping prompt",
                                node.label
                            );
                            continue;
                        }

                        // Build decision options from agent children only
                        let options: Vec<DecisionOption> = kids
                            .iter()
                            .filter_map(|kid_id| {
                                let kid = node_map.get(kid_id)?;
                                let kt = kid.node_type.as_deref().unwrap_or("agent");
                                if kt == "merge" || kt == "final" {
                                    return None;
                                }
                                Some(DecisionOption {
                                    node_id: kid.id.clone(),
                                    label: kid.label.clone(),
                                    prompt: kid.prompt.clone(),
                                })
                            })
                            .collect();

                        if options.is_empty() {
                            continue;
                        }

                        let decision = PendingDecision {
                            node_id: node_id.clone(),
                            label: node.label.clone(),
                            prompt: node.prompt.clone(),
                            options,
                        };

                        // Update status to waiting
                        {
                            let mut s = status.lock().await;
                            s.state = OrchestratorState::WaitingUser;
                            s.pending_decision = Some(decision.clone());
                        }

                        // Persist state
                        {
                            let db_c = db.clone();
                            let sid = session_id.to_string();
                            let nid = node_id.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                let conn = db_c.lock().ok()?;
                                db::orchestrator_update_state(
                                    &conn,
                                    &sid,
                                    "waiting_user",
                                    Some(&nid),
                                )
                                .ok()
                            })
                            .await;
                        }

                        // Emit decision needed event
                        let _ = app.emit(
                            "orchestrator_decision_needed",
                            OrchestratorDecisionPayload {
                                session_id: session_id.to_string(),
                                decision,
                            },
                        );

                        // Create a oneshot channel and store it in the handle
                        let (tx, rx) = oneshot::channel::<String>();
                        {
                            let mut active = active_ref.lock().await;
                            if let Some(handle) = active.get_mut(session_id) {
                                handle.decision_tx = Some(tx);
                            }
                        }

                        // Wait for user decision or cancellation
                        let _selected = tokio::select! {
                            result = rx => {
                                result.map_err(|_| "Decision channel closed".to_string())?
                            }
                            _ = &mut cancel_rx => {
                                return Err("Orchestrator cancelled".to_string());
                            }
                        };

                        // TODO: In supervised mode, skip non-selected branches.
                        // For now all branches run since they're linearized.
                    }
                }
            }

            "merge" | "final" => {
                // Merge and final nodes are runnable — they spawn a real agent process
                // with full TOON context (ancestor chain + sibling results).
                // Merge evaluates sibling agent outputs; final executes the culminating step.
                if node.status == NodeStatus::Pending {
                    run_single_node(node_id, &db, &pty, &sdk, &app).await?;

                    wait_for_completion(node_id, pty_rx, sdk_rx, &mut cancel_rx).await?;

                    {
                        let mut s = status.lock().await;
                        s.completed_count += 1;
                    }

                    let s = status.lock().await;
                    let _ = app.emit(
                        "orchestrator_progress",
                        OrchestratorProgressPayload {
                            session_id: session_id.to_string(),
                            node_id: node_id.clone(),
                            status: "completed".to_string(),
                            completed_count: s.completed_count,
                            total_count: s.total_count,
                        },
                    );
                    drop(s);

                    log::info!("Orchestrator completed {} node: {}", node_type, node.label);
                }
            }

            _ => {
                log::warn!(
                    "Unknown node type '{}' in orchestrator, skipping",
                    node_type
                );
            }
        }
    }

    Ok(())
}

// ─── Helper: Run a single node ──────────────────────────────────

async fn run_single_node(
    node_id: &str,
    db: &Arc<Mutex<Connection>>,
    pty: &Arc<PtyManager>,
    sdk: &Arc<SdkManager>,
    app: &AppHandle,
) -> Result<(), String> {
    let db2 = db.clone();
    let nid = node_id.to_string();

    // Fetch node
    let node = tokio::task::spawn_blocking({
        let db_c = db.clone();
        let nid = nid.clone();
        move || {
            let conn = db_c.lock().map_err(|e| format!("DB lock: {e}"))?;
            db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
        }
    })
    .await
    .map_err(|e| format!("Task: {e}"))??;

    // Fetch project
    let project = tokio::task::spawn_blocking({
        let db_c = db.clone();
        let pid = node.project_id.clone();
        move || {
            let conn = db_c.lock().map_err(|e| format!("DB lock: {e}"))?;
            db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))
        }
    })
    .await
    .map_err(|e| format!("Task: {e}"))??;

    // Ensure git repo
    let repo_path = project.repo_path.clone();
    tokio::task::spawn_blocking(move || {
        crate::git_manager::ensure_git_repo(&repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task: {e}"))??;

    // Determine base commit
    let from_commit = if node.parent_id.is_none() {
        None
    } else {
        let db_walk = db.clone();
        let mut current_id = node.parent_id.clone();
        let mut found_commit: Option<String> = None;
        while let Some(pid) = current_id {
            let db_inner = db_walk.clone();
            let pid_clone = pid.clone();
            let ancestor = tokio::task::spawn_blocking(move || {
                let conn = db_inner.lock().map_err(|e| format!("DB lock: {e}"))?;
                db::node_get_by_id(&conn, &pid_clone).map_err(|e| format!("{e}"))
            })
            .await
            .map_err(|e| format!("Task: {e}"))??;
            if let Some(ref hash) = ancestor.commit_hash {
                found_commit = Some(hash.clone());
                break;
            }
            current_id = ancestor.parent_id;
        }
        found_commit
    };

    // Create worktree
    let branch_name = format!(
        "crongen/{}/{}",
        node.label
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-'),
        db::now_unix()
    );
    let repo_path = project.repo_path.clone();
    let branch = branch_name.clone();
    let commit_ref = from_commit.clone();
    let wt_info = tokio::task::spawn_blocking(move || {
        git_manager::create_worktree(&repo_path, &branch, commit_ref.as_deref())
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task: {e}"))??;

    // Get commit hash
    let wt_path = wt_info.path.clone();
    let commit_hash = tokio::task::spawn_blocking(move || {
        git_manager::get_current_commit(&wt_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task: {e}"))??;

    // Update node in DB with worktree info
    {
        let db_c = db.clone();
        let nid = nid.clone();
        let bn = branch_name.clone();
        let wtp = wt_info.path.clone();
        let ch = commit_hash.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db_c.lock().map_err(|e| format!("DB lock: {e}"))?;
            conn.execute(
                "UPDATE decision_nodes SET branch_name=?1, worktree_path=?2, commit_hash=?3, updated_at=?4 WHERE id=?5",
                rusqlite::params![bn, Some(wtp), Some(ch), db::now_unix(), nid],
            )
            .map_err(|e| format!("DB: {e}"))?;
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("Task: {e}"))??;
    }

    // Build TOON context
    let toon_context = {
        let db_c = db.clone();
        let node_for_ctx = node.clone();
        let repo_path_for_ctx = project.repo_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db_c.lock().map_err(|e| format!("DB lock: {e}"))?;
            let ctx =
                context::build_execution_context(&conn, &node_for_ctx, Some(&repo_path_for_ctx))
                    .map_err(|e| format!("Context: {e}"))?;
            toon::build_context_string(&ctx)
        })
        .await
        .map_err(|e| format!("Task: {e}"))??
    };

    // Load execution model from settings
    let exec_model = crate::commands::get_settings()
        .await
        .ok()
        .and_then(|s| s.execution_model);

    // Build execution command
    let execution = agent_templates::build_shell_command(
        &project.agent_type,
        &node.prompt,
        &project.type_config,
        Some(&toon_context),
        node.node_type.as_deref(),
        exec_model.as_deref(),
    );

    pty.clear_session_artifacts(&nid);
    sdk.clear_session_artifacts(&nid);

    // Spawn the process
    match execution {
        ExecutionMode::Pty(shell) => {
            pty.spawn_session(
                &nid,
                &project.id,
                &nid,
                &shell.program,
                &shell.args,
                &wt_info.path,
                shell.stdin_injection.as_deref(),
                shell.auto_responses,
                db2,
                app.clone(),
            )
            .map_err(|e| format!("PTY spawn: {e}"))?;
        }
        ExecutionMode::Sdk(sdk_exec) => {
            sdk.spawn_session(
                &nid,
                &project.id,
                &nid,
                &sdk_exec.program,
                &sdk_exec.args,
                &wt_info.path,
                db2,
                app.clone(),
            )
            .map_err(|e| format!("SDK spawn: {e}"))?;
        }
    }

    log::info!("Orchestrator spawned node: {} ({})", node.label, nid);
    Ok(())
}

// ─── Helper: Wait for node completion ───────────────────────────

async fn wait_for_completion(
    node_id: &str,
    pty_rx: &mut broadcast::Receiver<SessionCompletion>,
    sdk_rx: &mut broadcast::Receiver<SessionCompletion>,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> Result<(), String> {
    loop {
        tokio::select! {
            result = pty_rx.recv() => {
                match result {
                    Ok(completion) if completion.node_id == node_id => {
                        return match completion.exit_code {
                            Some(0) => Ok(()),
                            Some(code) => Err(format!("Node {} failed with exit code {}", node_id, code)),
                            None => Err(format!("Node {} exited without status", node_id)),
                        };
                    }
                    Ok(_) => continue, // Different node
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err("PTY completion channel closed".to_string());
                    }
                }
            }
            result = sdk_rx.recv() => {
                match result {
                    Ok(completion) if completion.node_id == node_id => {
                        return match completion.exit_code {
                            Some(0) => Ok(()),
                            Some(code) => Err(format!("Node {} failed with exit code {}", node_id, code)),
                            None => Err(format!("Node {} exited without status", node_id)),
                        };
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err("SDK completion channel closed".to_string());
                    }
                }
            }
            _ = &mut *cancel_rx => {
                return Err("Orchestrator cancelled".to_string());
            }
        }
    }
}
