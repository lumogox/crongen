use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeInfo {
    pub path: String,
    pub branch_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub success: bool,
    pub merge_commit_hash: Option<String>,
    pub conflict_files: Vec<String>,
    /// Set when conflicts were auto-resolved by an agent.
    pub auto_resolved: bool,
    /// Summary from the resolution agent (if auto-resolved).
    pub resolution_summary: Option<String>,
}

/// Preview of what a merge would look like (before actually merging).
#[derive(Debug, Clone, Serialize)]
pub struct MergePreview {
    pub source_branch: String,
    pub target_branch: String,
    pub commit_count: usize,
    pub files_changed: Vec<String>,
}

// ─── Validation ───────────────────────────────────────────────

/// Ensure a directory is a git repository with at least one commit.
/// If no repo exists, runs `git init`. If the repo is empty (no commits),
/// creates an initial commit so that worktrees can branch from HEAD.
pub fn ensure_git_repo(path: &str) -> Result<()> {
    let repo = match git2::Repository::open(path) {
        Ok(repo) => {
            if repo.is_bare() {
                bail!("Bare repositories are not supported: {path}");
            }
            repo
        }
        Err(_) => {
            log::info!("No git repo found at {path}, initializing...");
            git2::Repository::init(path)
                .with_context(|| format!("Failed to initialize git repository at {path}"))?
        }
    };

    // Check if the repo has any commits (HEAD exists and points to a commit)
    if repo.head().is_err() {
        log::info!("Empty git repo at {path}, creating initial commit...");

        // Add .agent-chron-worktrees to .gitignore so worktrees don't get tracked
        let gitignore_path = Path::new(path).join(".gitignore");
        let mut gitignore_content = String::new();
        if gitignore_path.exists() {
            gitignore_content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        }
        if !gitignore_content.contains(".agent-chron-worktrees") {
            if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
                gitignore_content.push('\n');
            }
            gitignore_content.push_str(".agent-chron-worktrees\n");
            std::fs::write(&gitignore_path, &gitignore_content)
                .context("Failed to write .gitignore")?;
        }

        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("Agent-Chron", "agent-chron@local"))
            .context("Failed to create git signature")?;

        // Stage .gitignore and create initial commit
        let mut index = repo.index().context("Failed to get repo index")?;
        index
            .add_path(Path::new(".gitignore"))
            .context("Failed to stage .gitignore")?;
        index.write().context("Failed to write index")?;
        let tree_id = index.write_tree().context("Failed to write tree")?;
        let tree = repo
            .find_tree(tree_id)
            .context("Failed to find initial tree")?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial commit (Agent-Chron)",
            &tree,
            &[],
        )
        .context("Failed to create initial commit")?;
        log::info!("Created initial commit at {path}");
    }

    Ok(())
}

// ─── Commit Info ──────────────────────────────────────────────

/// Get the HEAD commit hash for a repository or worktree path.
pub fn get_current_commit(path: &str) -> Result<String> {
    let repo = git2::Repository::open(path)
        .with_context(|| format!("Failed to open repository: {path}"))?;

    let head = repo.head().context("Failed to get HEAD reference")?;
    let commit = head
        .peel_to_commit()
        .context("HEAD does not point to a commit")?;

    Ok(commit.id().to_string())
}

/// Get the default branch name (main or master) for a repository.
pub fn get_default_branch(repo_path: &str) -> Result<String> {
    let repo = git2::Repository::open(repo_path).context("Failed to open repository")?;

    if repo.find_branch("main", git2::BranchType::Local).is_ok() {
        Ok("main".to_string())
    } else if repo.find_branch("master", git2::BranchType::Local).is_ok() {
        Ok("master".to_string())
    } else {
        // Fall back to whatever HEAD points to
        let head = repo.head().context("Failed to get HEAD")?;
        head.shorthand()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not determine default branch"))
    }
}

// ─── Worktree Directory ──────────────────────────────────────

/// Worktrees are stored inside the repo:
///   {repo_path}/.agent-chron-worktrees/{branch_name}
fn worktrees_dir(repo_path: &str) -> PathBuf {
    Path::new(repo_path).join(".agent-chron-worktrees")
}

// ─── Worktree Create ─────────────────────────────────────────

/// Create a git worktree on a new branch, optionally from a specific commit.
///
/// - `repo_path`: path to the main git repository
/// - `branch_name`: name for the new branch (e.g. "agent-chron/nightly-tests/1709...")
/// - `from_commit`: optional commit SHA to branch from (defaults to HEAD)
pub fn create_worktree(
    repo_path: &str,
    branch_name: &str,
    from_commit: Option<&str>,
) -> Result<WorktreeInfo> {
    let wt_dir = worktrees_dir(repo_path);
    std::fs::create_dir_all(&wt_dir)
        .with_context(|| format!("Failed to create worktrees directory: {}", wt_dir.display()))?;

    let wt_path = wt_dir.join(branch_name.replace('/', "-"));
    if wt_path.exists() {
        bail!("Worktree already exists: {}", wt_path.display());
    }

    let wt_path_str = wt_path.to_string_lossy().to_string();

    let mut args = vec![
        "worktree".to_string(),
        "add".to_string(),
        "-b".to_string(),
        branch_name.to_string(),
        wt_path_str.clone(),
    ];

    if let Some(commit) = from_commit {
        args.push(commit.to_string());
    }

    let output = Command::new("git")
        .current_dir(repo_path)
        .args(&args)
        .output()
        .context("Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree add failed: {stderr}");
    }

    log::info!("Created worktree: {wt_path_str} on branch {branch_name}");

    Ok(WorktreeInfo {
        path: wt_path_str,
        branch_name: branch_name.to_string(),
    })
}

// ─── Worktree Remove ─────────────────────────────────────────

/// Remove a git worktree and optionally delete the associated branch.
pub fn remove_worktree(repo_path: &str, worktree_path: &str, delete_branch: bool) -> Result<()> {
    // Read the branch name before removal (if we need to delete it)
    let branch_name = if delete_branch {
        git2::Repository::open(worktree_path)
            .ok()
            .and_then(|r| r.head().ok()?.shorthand().map(|s| s.to_string()))
    } else {
        None
    };

    // Attempt to remove the worktree via git CLI
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["worktree", "remove", "--force", worktree_path])
        .output()
        .context("Failed to execute git worktree remove")?;

    if !output.status.success() {
        // If git worktree remove fails, prune stale entries and clean up manually
        let _ = Command::new("git")
            .current_dir(repo_path)
            .args(["worktree", "prune"])
            .output();

        if Path::new(worktree_path).exists() {
            std::fs::remove_dir_all(worktree_path)
                .with_context(|| format!("Failed to remove worktree directory: {worktree_path}"))?;
        }
    }

    // Delete the branch from the main repo if requested
    if let Some(branch) = branch_name {
        let _ = Command::new("git")
            .current_dir(repo_path)
            .args(["branch", "-D", &branch])
            .output();
        log::info!("Deleted branch: {branch}");
    }

    log::info!("Removed worktree: {worktree_path}");
    Ok(())
}

// ─── Branch Diff ─────────────────────────────────────────────

/// Compute a diff between a base commit and a branch tip.
/// Returns (diff_stat, diff_content) where diff_content is truncated at max_chars.
pub fn get_branch_diff(
    repo_path: &str,
    base_commit: &str,
    branch_name: &str,
    max_chars: usize,
) -> Result<(String, String)> {
    // Get --stat summary
    let stat_output = Command::new("git")
        .current_dir(repo_path)
        .args(["diff", "--stat", &format!("{base_commit}..{branch_name}")])
        .output()
        .with_context(|| format!("Failed to run git diff --stat for branch {branch_name}"))?;

    if !stat_output.status.success() {
        let stderr = String::from_utf8_lossy(&stat_output.stderr);
        bail!("git diff --stat failed for {branch_name}: {stderr}");
    }

    let diff_stat = String::from_utf8_lossy(&stat_output.stdout).to_string();

    // Get full diff content
    let diff_output = Command::new("git")
        .current_dir(repo_path)
        .args(["diff", &format!("{base_commit}..{branch_name}")])
        .output()
        .with_context(|| format!("Failed to run git diff for branch {branch_name}"))?;

    if !diff_output.status.success() {
        let stderr = String::from_utf8_lossy(&diff_output.stderr);
        bail!("git diff failed for {branch_name}: {stderr}");
    }

    let mut diff_content = String::from_utf8_lossy(&diff_output.stdout).to_string();
    if diff_content.len() > max_chars {
        diff_content.truncate(max_chars);
        diff_content.push_str("\n... [truncated]");
    }

    Ok((diff_stat, diff_content))
}

// ─── Auto-commit ─────────────────────────────────────────────

/// Commit any uncommitted changes in a worktree.
/// Agents don't always commit their work before exiting, so this ensures
/// all changes are captured on the branch before merging.
/// Returns true if a commit was created, false if the worktree was clean.
pub fn auto_commit_worktree(worktree_path: &str) -> Result<bool> {
    // Check if there are any changes (staged, unstaged, or untracked)
    let status = Command::new("git")
        .current_dir(worktree_path)
        .args(["status", "--porcelain"])
        .output()
        .context("Failed to check git status in worktree")?;

    let status_output = String::from_utf8_lossy(&status.stdout);
    if status_output.trim().is_empty() {
        return Ok(false); // Clean worktree, nothing to commit
    }

    log::info!(
        "Auto-committing uncommitted changes in worktree: {}",
        worktree_path
    );

    // Stage all changes (including untracked files)
    let add = Command::new("git")
        .current_dir(worktree_path)
        .args(["add", "-A"])
        .output()
        .context("Failed to stage changes in worktree")?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        bail!("git add -A failed in worktree: {stderr}");
    }

    // Commit with a descriptive message
    let commit = Command::new("git")
        .current_dir(worktree_path)
        .args([
            "commit",
            "-m",
            "Auto-commit agent work (uncommitted changes captured by Agent-Chron)",
        ])
        .output()
        .context("Failed to commit in worktree")?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        // "nothing to commit" is not a real error
        if stderr.contains("nothing to commit") {
            return Ok(false);
        }
        bail!("git commit failed in worktree: {stderr}");
    }

    log::info!("Auto-committed changes in worktree: {}", worktree_path);
    Ok(true)
}

// ─── Branch Info ─────────────────────────────────────────────

/// Create a new branch pointing at a specific commit.
/// Returns the full branch name.
pub fn create_branch_at(repo_path: &str, branch_name: &str, commit_sha: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["branch", branch_name, commit_sha])
        .output()
        .with_context(|| format!("Failed to create branch {branch_name}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch failed: {stderr}");
    }

    log::info!("Created branch {branch_name} at {commit_sha}");
    Ok(branch_name.to_string())
}

// ─── Merge ───────────────────────────────────────────────────

/// Merge a source branch into a target branch (defaults to main/master).
///
/// The merge is performed in the main repo directory. If conflicts arise,
/// the merge is aborted and conflict file names are returned.
pub fn merge_branch(
    repo_path: &str,
    source_branch: &str,
    target_branch: Option<&str>,
) -> Result<MergeResult> {
    let target = match target_branch {
        Some(t) => t.to_string(),
        None => get_default_branch(repo_path)?,
    };

    // Checkout target branch
    let checkout = Command::new("git")
        .current_dir(repo_path)
        .args(["checkout", &target])
        .output()
        .context("Failed to execute git checkout")?;

    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr);
        bail!("Failed to checkout {target}: {stderr}");
    }

    // Attempt the merge
    let merge = Command::new("git")
        .current_dir(repo_path)
        .args(["merge", source_branch, "--no-edit"])
        .output()
        .context("Failed to execute git merge")?;

    let merge_stdout = String::from_utf8_lossy(&merge.stdout);
    let merge_stderr = String::from_utf8_lossy(&merge.stderr);

    // "Already up to date" is technically success — git exits 0 for this,
    // but check stdout just in case some git versions differ
    if merge.status.success() || merge_stdout.contains("Already up to date") {
        let commit_hash = get_current_commit(repo_path).ok();
        log::info!("Merged {source_branch} into {target}");

        Ok(MergeResult {
            success: true,
            merge_commit_hash: commit_hash,
            conflict_files: vec![],
            auto_resolved: false,
            resolution_summary: None,
        })
    } else {
        // Identify conflicting files
        let conflicts = get_conflict_files(repo_path)?;

        // If no conflict files found, this is a non-conflict error (branch missing, etc.)
        if conflicts.is_empty() {
            let err_msg = if !merge_stderr.is_empty() {
                merge_stderr.trim().to_string()
            } else if !merge_stdout.is_empty() {
                merge_stdout.trim().to_string()
            } else {
                format!("git merge failed for branch {source_branch}")
            };
            abort_merge(repo_path);
            bail!("{err_msg}");
        }

        log::warn!(
            "Merge {source_branch} into {target} failed with {} conflicts",
            conflicts.len()
        );

        Ok(MergeResult {
            success: false,
            merge_commit_hash: None,
            conflict_files: conflicts,
            auto_resolved: false,
            resolution_summary: None,
        })
    }
}

/// List files with merge conflicts in the current repo state.
pub fn get_conflict_files(repo_path: &str) -> Result<Vec<String>> {
    let diff_output = Command::new("git")
        .current_dir(repo_path)
        .args(["diff", "--name-only", "--diff-filter=U"])
        .output()
        .context("Failed to list merge conflicts")?;

    Ok(String::from_utf8_lossy(&diff_output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Abort a merge in progress. Leaves the repo clean.
pub fn abort_merge(repo_path: &str) {
    let _ = Command::new("git")
        .current_dir(repo_path)
        .args(["merge", "--abort"])
        .output();
}

// ─── Merge Preview ────────────────────────────────────────────

/// Compute a preview of what merging source_branch into target would look like.
pub fn get_merge_preview(
    repo_path: &str,
    source_branch: &str,
    target_branch: Option<&str>,
) -> Result<MergePreview> {
    let target = match target_branch {
        Some(t) => t.to_string(),
        None => get_default_branch(repo_path)?,
    };

    // Count commits between target and source
    let rev_list = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-list", "--count", &format!("{target}..{source_branch}")])
        .output()
        .context("Failed to count commits")?;

    let commit_count = if rev_list.status.success() {
        String::from_utf8_lossy(&rev_list.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    } else {
        0
    };

    // List changed files (three-dot diff = changes since branches diverged)
    let diff = Command::new("git")
        .current_dir(repo_path)
        .args([
            "diff",
            "--name-only",
            &format!("{target}...{source_branch}"),
        ])
        .output()
        .context("Failed to list changed files")?;

    let files_changed = if diff.status.success() {
        String::from_utf8_lossy(&diff.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect()
    } else {
        vec![]
    };

    Ok(MergePreview {
        source_branch: source_branch.to_string(),
        target_branch: target,
        commit_count,
        files_changed,
    })
}

/// After conflicts have been resolved, stage and commit.
pub fn finalize_merge_resolution(repo_path: &str) -> Result<String> {
    let add = Command::new("git")
        .current_dir(repo_path)
        .args(["add", "-A"])
        .output()
        .context("Failed to stage resolved files")?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        bail!("git add failed: {stderr}");
    }

    let commit = Command::new("git")
        .current_dir(repo_path)
        .args(["commit", "--no-edit"])
        .output()
        .context("Failed to commit merge resolution")?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        bail!("git commit failed: {stderr}");
    }

    get_current_commit(repo_path)
}
