use rusqlite::Connection;
use serde::Serialize;

use crate::db;
use crate::git_manager;
use crate::models::DecisionNode;

// ─── Context Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AncestorStep {
    pub node_type: String,
    pub label: String,
    pub prompt: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiblingInfo {
    pub label: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiblingDiff {
    pub label: String,
    pub branch_name: String,
    pub status: String,
    pub diff_stat: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParentDiff {
    pub label: String,
    pub node_type: String,
    pub diff_stat: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContext {
    pub session_label: String,
    pub session_goal: String,
    pub ancestor_path: Vec<AncestorStep>,
    pub current_node: AncestorStep,
    pub sibling_info: Vec<SiblingInfo>,
    pub sibling_diffs: Vec<SiblingDiff>,
    pub parent_diff: Option<ParentDiff>,
    pub directive: Option<String>,
}

// ─── Builder Functions ─────────────────────────────────────────

/// Walk up the parent chain from a node to the root, collecting ancestors.
/// Returns ancestors in root-first order (root at index 0).
pub fn build_ancestor_chain(
    conn: &Connection,
    node: &DecisionNode,
) -> anyhow::Result<Vec<DecisionNode>> {
    let mut chain = Vec::new();
    let mut current_id = node.parent_id.clone();

    while let Some(pid) = current_id {
        let parent = db::node_get_by_id(conn, &pid)?;
        current_id = parent.parent_id.clone();
        chain.push(parent);
    }

    chain.reverse();
    Ok(chain)
}

/// Build a full execution context for a node about to be executed.
/// This includes the session info, ancestor chain, current node details,
/// sibling status (other children of the same parent), and for merge/final
/// nodes: git diffs from completed sibling branches.
pub fn build_execution_context(
    conn: &Connection,
    node: &DecisionNode,
    repo_path: Option<&str>,
) -> anyhow::Result<ExecutionContext> {
    let ancestors = build_ancestor_chain(conn, node)?;

    // Session info comes from the root node (first ancestor or self if root)
    let (session_label, session_goal) = if let Some(root) = ancestors.first() {
        (root.label.clone(), root.prompt.clone())
    } else {
        (node.label.clone(), node.prompt.clone())
    };

    // Convert ancestors to steps
    let ancestor_path: Vec<AncestorStep> = ancestors
        .iter()
        .map(|a| AncestorStep {
            node_type: a.node_type.clone().unwrap_or_else(|| "task".to_string()),
            label: a.label.clone(),
            prompt: a.prompt.clone(),
            status: a.status.as_str().to_string(),
        })
        .collect();

    // Current node step
    let current_node = AncestorStep {
        node_type: node
            .node_type
            .clone()
            .unwrap_or_else(|| "agent".to_string()),
        label: node.label.clone(),
        prompt: node.prompt.clone(),
        status: node.status.as_str().to_string(),
    };

    // Sibling info: other children of the same parent
    let siblings = if let Some(ref pid) = node.parent_id {
        db::node_get_children(conn, pid)?
    } else {
        Vec::new()
    };

    let sibling_info: Vec<SiblingInfo> = siblings
        .iter()
        .filter(|s| s.id != node.id)
        .map(|s| SiblingInfo {
            label: s.label.clone(),
            status: s.status.as_str().to_string(),
            exit_code: s.exit_code,
        })
        .collect();

    // Sibling diffs: for merge/final nodes, include git diffs from completed siblings
    let node_type_str = node.node_type.as_deref().unwrap_or("agent");
    let sibling_diffs = if (node_type_str == "merge" || node_type_str == "final")
        && repo_path.is_some()
    {
        let repo = repo_path.unwrap();

        // Find base commit: nearest ancestor with a commit_hash
        let base_commit = ancestors
            .iter()
            .rev()
            .find_map(|a| a.commit_hash.clone())
            .or_else(|| node.commit_hash.clone());

        if let Some(base) = base_commit {
            siblings
                .iter()
                .filter(|s| {
                    s.id != node.id && s.status.as_str() == "completed" && !s.branch_name.is_empty()
                })
                .filter_map(|s| {
                    match git_manager::get_branch_diff(repo, &base, &s.branch_name, 10_000) {
                        Ok((stat, _content)) => Some(SiblingDiff {
                            label: s.label.clone(),
                            branch_name: s.branch_name.clone(),
                            status: s.status.as_str().to_string(),
                            diff_stat: stat,
                        }),
                        Err(e) => {
                            log::warn!(
                                "Could not get diff for sibling '{}' branch '{}': {e}",
                                s.label,
                                s.branch_name
                            );
                            None
                        }
                    }
                })
                .collect()
        } else {
            log::warn!(
                "No base commit found for merge/final node '{}' — skipping sibling diffs",
                node.label
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Parent diff: show what the immediate parent node changed (if it has commits)
    let parent_diff = if let Some(ref pid) = node.parent_id {
        let parent = db::node_get_by_id(conn, pid)?;
        if let (Some(ref parent_hash), Some(repo)) = (&parent.commit_hash, repo_path) {
            // Find grandparent's commit (parent's base)
            let grandparent_hash = if let Some(ref gpid) = parent.parent_id {
                let gp = db::node_get_by_id(conn, gpid)?;
                gp.commit_hash.clone()
            } else {
                None
            };
            if let Some(ref base) = grandparent_hash {
                match git_manager::get_branch_diff(repo, base, parent_hash, 5_000) {
                    Ok((stat, _content)) => Some(ParentDiff {
                        label: parent.label.clone(),
                        node_type: parent.node_type.clone().unwrap_or_else(|| "task".into()),
                        diff_stat: stat,
                    }),
                    Err(e) => {
                        log::warn!("Parent diff failed: {e}");
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Directive: role-framing based on position in the execution tree
    let directive = {
        let parent_type = node
            .parent_id
            .as_ref()
            .and_then(|pid| db::node_get_by_id(conn, pid).ok())
            .and_then(|p| p.node_type);

        // Check if this node has a decision child (meaning it's a scaffolding step)
        let has_decision_child = {
            let children = db::node_get_children(conn, &node.id).unwrap_or_default();
            children
                .iter()
                .any(|c| c.node_type.as_deref() == Some("decision"))
        };

        let my_type = node.node_type.as_deref().unwrap_or("agent");

        if has_decision_child {
            // Check if this is a subsequent session on a project that was scaffolded previously.
            // If other root sessions already completed, skip scaffolding — treat as existing code.
            let is_subsequent_session = {
                let roots = db::node_get_roots(conn, &node.project_id).unwrap_or_default();
                roots.iter().any(|r| {
                    r.id != node.id
                        && (r.status == crate::models::NodeStatus::Completed
                            || r.status == crate::models::NodeStatus::Merged)
                })
            };

            if is_subsequent_session {
                Some(
                    "This project already has existing code from prior sessions. \
                     Analyze the current codebase to understand the architecture, conventions, and key files. \
                     Do NOT scaffold, re-initialize, or create a new project. \
                     Downstream agents will implement changes within the existing codebase."
                        .to_string(),
                )
            } else {
                Some(
                    "This is a SCAFFOLDING step only. Set up the project structure, install dependencies, \
                     and create config files. Do NOT implement features, make design choices, or pick approaches. \
                     Downstream agents will handle implementation in separate branches."
                        .to_string(),
                )
            }
        } else if my_type == "merge" {
            // Merge nodes evaluate siblings and merge the winner — their own directive
            // comes from agent_templates (MERGE PROCEDURE). No extra directive needed here.
            None
        } else {
            match parent_type.as_deref() {
                Some("merge") => Some(
                    "The previous step already evaluated alternatives and merged the winning code. \
                     Your worktree contains the chosen implementation. Do NOT re-evaluate prior decisions. \
                     If a DECISION.md file exists, read it for rationale. \
                     Build on the existing code to complete your specific task."
                        .to_string(),
                ),
                Some("decision") => Some(
                    "You are implementing ONE specific variation as part of a comparison. \
                     Your worktree already contains work from ancestor tasks — do NOT re-scaffold, \
                     re-initialize, or create a new project from scratch. Build on what already exists. \
                     Focus exclusively on implementing your specific approach/design. \
                     Your work will be compared against sibling alternatives."
                        .to_string(),
                ),
                _ => None,
            }
        }
    };

    Ok(ExecutionContext {
        session_label,
        session_goal,
        ancestor_path,
        current_node,
        sibling_info,
        sibling_diffs,
        parent_diff,
        directive,
    })
}
