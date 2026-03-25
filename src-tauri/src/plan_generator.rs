use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::db;
use crate::models::{AgentType, DecisionNode, NodeStatus};

// ─── Plan Types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNode {
    pub label: String,
    pub prompt: String,
    pub node_type: String, // "task", "decision", "agent", "merge", "final"
    #[serde(default)]
    pub children: Vec<PlanNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPlan {
    pub root: PlanNode,
}

// ─── System Prompt ──────────────────────────────────────────────

const PLAN_SYSTEM_PROMPT: &str = r#"You are a task decomposition planner. Output ONLY raw JSON, no markdown fences, no explanation.

Rules:
- Root node: type "task" (one child: a "decision" or "agent")
- "decision" nodes contain 2+ "agent" children PLUS a "merge" child (last)
- "merge" nodes contain one "final" child
- The tree is NESTED: task > decision > [agents..., merge > final]
- Max 8 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo System", "Auth Login Page"), NOT the structural role. Child labels describe what each step does specifically.

CRITICAL prompt scoping rules:
- The root "task" prompt ONLY handles scaffolding: project init, install dependencies, create config files, set up folder structure. It must NOT make design choices, pick approaches, or implement features.
- Agent prompts under a "decision" MUST say "Implement/Apply X to the existing project" — they modify the codebase left by the root task, they do NOT scaffold or create a new project.
- The "merge" prompt evaluates/compares the agent branches.
- The "final" prompt applies finishing touches to the chosen result — it does NOT re-scaffold.

Example for user task "Build a calculator with theme support":
{"root":{"label":"Calculator Themes","prompt":"Scaffold a new Vite + TypeScript project, install dependencies, and create the basic folder structure. Do NOT implement any features or make design choices.","node_type":"task","children":[{"label":"Theme Strategy","prompt":"Pick theme implementation strategy","node_type":"decision","children":[{"label":"CSS Variables","prompt":"Implement theming using CSS custom properties in the existing project","node_type":"agent","children":[]},{"label":"Tailwind Dark Mode","prompt":"Implement theming using Tailwind dark mode classes in the existing project","node_type":"agent","children":[]},{"label":"Evaluate Themes","prompt":"Evaluate both theme approaches","node_type":"merge","children":[{"label":"Polish UI","prompt":"Polish and finalize the chosen theme approach","node_type":"final","children":[]}]}]}]}}"#;

const PLAN_SYSTEM_PROMPT_EXISTING: &str = r#"You are a task decomposition planner for an EXISTING codebase. Output ONLY raw JSON, no markdown fences, no explanation.

Rules:
- Root node: type "task" (one child: a "decision" or "agent")
- "decision" nodes contain 2+ "agent" children PLUS a "merge" child (last)
- "merge" nodes contain one "final" child
- The tree is NESTED: task > decision > [agents..., merge > final]
- Max 8 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo System", "Dark Mode Toggle"), NOT the structural role. Child labels describe what each step does specifically.

CRITICAL prompt scoping rules for EXISTING projects:
- The root "task" prompt reads and analyzes the existing codebase — it does NOT scaffold, create a new project, or re-initialize anything. It should understand the project structure, conventions, and key files before implementation begins.
- Agent prompts under a "decision" MUST build on the existing code. They implement the requested feature/change within the existing architecture and conventions.
- The "merge" prompt evaluates/compares the agent branches.
- The "final" prompt integrates the chosen approach, updates tests, and ensures consistency with the rest of the codebase.

Example for user task "Add undo/redo to the calculator":
{"root":{"label":"Undo/Redo System","prompt":"Read the existing project structure, understand the architecture and state management. Do NOT create new projects or re-scaffold.","node_type":"task","children":[{"label":"Undo Strategy","prompt":"Pick undo/redo implementation strategy","node_type":"decision","children":[{"label":"History Stack","prompt":"Implement undo/redo using a history stack array in the existing codebase","node_type":"agent","children":[]},{"label":"Command Pattern","prompt":"Implement undo/redo using the command pattern in the existing codebase","node_type":"agent","children":[]},{"label":"Compare Approaches","prompt":"Evaluate both undo/redo implementations","node_type":"merge","children":[{"label":"Finalize Undo/Redo","prompt":"Integrate the chosen approach, add keyboard shortcuts, and test","node_type":"final","children":[]}]}]}]}}"#;

const PLAN_LINEAR_PROMPT: &str = r#"You are a task planner. Output ONLY raw JSON, no markdown fences, no explanation.

Rules for LINEAR plans (no branching, no decisions):
- Root node: type "task" — sets up the project
- The tree is a single chain of "agent" nodes: task > agent > agent > ...
- Each node has at most one child
- NO "decision", "merge", or "final" nodes
- Max 3 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Auth System", "Search Feature"), NOT the structural role. Child labels describe specific steps.

Example for user task "Build a todo list app":
{"root":{"label":"Todo List App","prompt":"Scaffold a new Vite + React project with Tailwind CSS.","node_type":"task","children":[{"label":"Build Todo UI","prompt":"Implement the todo list with add, complete, and delete functionality.","node_type":"agent","children":[{"label":"Polish Todo UI","prompt":"Refine the interface and add basic validation.","node_type":"agent","children":[]}]}]}}"#;

const PLAN_LINEAR_EXISTING_PROMPT: &str = r#"You are a task planner for an EXISTING codebase. Output ONLY raw JSON, no markdown fences, no explanation.

Rules for LINEAR plans (no branching, no decisions):
- Root node: type "task" — reads and understands the existing codebase (do NOT scaffold or re-init)
- The tree is a single chain of "agent" nodes: task > agent > agent > ...
- Each node has at most one child
- NO "decision", "merge", or "final" nodes
- Max 3 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo", "Dark Mode"), NOT the structural role. Child labels describe specific steps.

Example for user task "Add undo/redo to the calculator":
{"root":{"label":"Undo/Redo","prompt":"Read the project structure and understand how state is managed. Do NOT scaffold.","node_type":"task","children":[{"label":"Implement Undo/Redo","prompt":"Add undo/redo using a history stack, with Ctrl+Z/Ctrl+Y keyboard shortcuts.","node_type":"agent","children":[{"label":"Polish Undo/Redo","prompt":"Add tests and verify the implementation fits the existing codebase.","node_type":"agent","children":[]}]}]}}"#;

#[derive(Debug, Clone)]
struct PlannerInvocation {
    program: String,
    args: Vec<String>,
    current_dir: PathBuf,
    output_file: Option<PathBuf>,
    schema_file: Option<PathBuf>,
}

fn planner_provider_name(provider: &AgentType) -> &'static str {
    match provider {
        AgentType::ClaudeCode => "Claude",
        AgentType::Codex => "Codex",
        AgentType::Gemini => "Gemini",
        AgentType::Custom => "Custom provider",
    }
}

fn cleanup_output_file(output_file: Option<&PathBuf>) {
    if let Some(file) = output_file {
        let _ = fs::remove_file(file);
    }
}

fn write_plan_output_schema() -> Result<PathBuf, String> {
    let schema_file =
        std::env::temp_dir().join(format!("crongen-plan-schema-{}.json", uuid::Uuid::new_v4()));
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["root"],
        "properties": {
            "root": {
                "$ref": "#/$defs/planNode"
            }
        },
        "$defs": {
            "planNode": {
                "type": "object",
                "additionalProperties": false,
                "required": ["label", "prompt", "node_type", "children"],
                "properties": {
                    "label": { "type": "string" },
                    "prompt": { "type": "string" },
                    "node_type": {
                        "type": "string",
                        "enum": ["task", "decision", "agent", "merge", "final"]
                    },
                    "children": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/planNode" }
                    }
                }
            }
        }
    });

    let schema_json = serde_json::to_string_pretty(&schema)
        .map_err(|err| format!("Failed to serialize plan schema: {err}"))?;
    fs::write(&schema_file, schema_json)
        .map_err(|err| format!("Failed to write plan schema: {err}"))?;

    Ok(schema_file)
}

fn planner_system_prompt(project_mode: &str, complexity: &str) -> &'static str {
    match (complexity, project_mode) {
        ("linear", "existing") => PLAN_LINEAR_EXISTING_PROMPT,
        ("linear", _) => PLAN_LINEAR_PROMPT,
        (_, "existing") => PLAN_SYSTEM_PROMPT_EXISTING,
        _ => PLAN_SYSTEM_PROMPT,
    }
}

/// Normalize a generated plan for the requested complexity.
///
/// Linear plans are forced into a single chain so the canvas renders as a
/// straightforward top-down path instead of sibling branches.
pub fn normalize_plan_for_complexity(plan: GeneratedPlan, complexity: &str) -> GeneratedPlan {
    const MAX_LINEAR_AGENT_STEPS: usize = 2;

    if complexity != "linear" {
        return plan;
    }

    fn collect_linear_steps(node: &PlanNode, steps: &mut Vec<PlanNode>) {
        for child in &node.children {
            steps.push(PlanNode {
                label: child.label.clone(),
                prompt: child.prompt.clone(),
                node_type: "agent".to_string(),
                children: Vec::new(),
            });
            collect_linear_steps(child, steps);
        }
    }

    fn chain_linear_steps(steps: Vec<PlanNode>) -> Vec<PlanNode> {
        let mut next_child: Option<PlanNode> = None;

        for mut step in steps.into_iter().rev() {
            step.children = next_child.into_iter().collect();
            next_child = Some(step);
        }

        next_child.into_iter().collect()
    }

    let GeneratedPlan { root } = plan;
    let mut steps = Vec::new();
    collect_linear_steps(&root, &mut steps);
    let children = chain_linear_steps(steps.into_iter().take(MAX_LINEAR_AGENT_STEPS).collect());

    GeneratedPlan {
        root: PlanNode {
            label: root.label,
            prompt: root.prompt,
            node_type: "task".to_string(),
            children,
        },
    }
}

fn build_planner_invocation(
    provider: &AgentType,
    full_prompt: &str,
    model: Option<&str>,
    repo_path: &str,
) -> Result<PlannerInvocation, String> {
    match provider {
        AgentType::ClaudeCode => {
            let mut args = vec![
                "-p".to_string(),
                full_prompt.to_string(),
                "--output-format".to_string(),
                "text".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ];
            if let Some(m) = model {
                args.push("--model".to_string());
                args.push(m.to_string());
            }

            Ok(PlannerInvocation {
                program: "claude".to_string(),
                args,
                current_dir: PathBuf::from(repo_path),
                output_file: None,
                schema_file: None,
            })
        }
        AgentType::Codex => {
            let output_file =
                std::env::temp_dir().join(format!("crongen-plan-{}.txt", uuid::Uuid::new_v4()));
            let schema_file = write_plan_output_schema()?;
            let mut args = vec![
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "--sandbox".to_string(),
                "read-only".to_string(),
                "--output-last-message".to_string(),
                output_file.display().to_string(),
                "--cd".to_string(),
                repo_path.to_string(),
            ];
            args.push("--output-schema".to_string());
            args.push(schema_file.display().to_string());
            if let Some(m) = model {
                args.push("--model".to_string());
                args.push(m.to_string());

                if matches!(m, "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3") {
                    args.push("-c".to_string());
                    args.push("model_reasoning_effort=\"medium\"".to_string());
                }
            }
            args.push(full_prompt.to_string());

            Ok(PlannerInvocation {
                program: "codex".to_string(),
                args,
                current_dir: PathBuf::from(repo_path),
                output_file: Some(output_file),
                schema_file: Some(schema_file),
            })
        }
        AgentType::Gemini => Err("Gemini planning is coming soon.".to_string()),
        AgentType::Custom => Err("Custom providers are not supported for planning.".to_string()),
    }
}

// ─── Key Normalization ──────────────────────────────────────────

/// Recursively lowercase all object keys in a JSON value.
/// LLMs sometimes return "Label" instead of "label", etc.
fn normalize_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let normalized = map
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), normalize_keys(v)))
                .collect();
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(normalize_keys).collect())
        }
        other => other,
    }
}

/// Try to parse a JSON string as a GeneratedPlan, normalizing keys first.
fn try_parse_plan(json_str: &str) -> Option<GeneratedPlan> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let normalized = normalize_keys(value);
    serde_json::from_value(normalized).ok()
}

/// Extract the outermost JSON object from a string using brace counting.
/// Handles preamble text, markdown fences, trailing commentary, etc.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in text[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None // Unbalanced — JSON is truncated
}

// ─── Generator ──────────────────────────────────────────────────

/// Generate an execution plan by invoking the selected provider CLI.
/// Returns a parsed plan tree that can be converted to DecisionNodes.
pub async fn generate_plan(
    provider: &AgentType,
    prompt: &str,
    project_mode: &str,
    model: Option<&str>,
    complexity: &str,
    repo_path: &str,
) -> Result<GeneratedPlan, String> {
    let system_prompt = planner_system_prompt(project_mode, complexity);
    let full_prompt = format!("{system_prompt}\n\nUser task: {prompt}");
    let invocation = build_planner_invocation(provider, &full_prompt, model, repo_path)?;

    let output = Command::new(&invocation.program)
        .args(&invocation.args)
        .current_dir(&invocation.current_dir)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn {}: {e}", invocation.program))?;

    if !output.status.success() {
        cleanup_output_file(invocation.output_file.as_ref());
        cleanup_output_file(invocation.schema_file.as_ref());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(format!(
            "{} exited with error: {details}",
            planner_provider_name(provider)
        ));
    }

    let raw_output = if let Some(output_file) = &invocation.output_file {
        let contents = fs::read_to_string(output_file).map_err(|e| {
            cleanup_output_file(Some(output_file));
            cleanup_output_file(invocation.schema_file.as_ref());
            format!("Failed to read planner output: {e}")
        });
        cleanup_output_file(Some(output_file));
        let contents = contents?;
        cleanup_output_file(invocation.schema_file.as_ref());
        contents
    } else {
        cleanup_output_file(invocation.schema_file.as_ref());
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let stdout = raw_output.as_str();

    // Strategy 1: Extract outermost JSON object using brace counting.
    // This handles markdown fences, preamble text, trailing commentary, etc.
    // Key normalization handles Claude's inconsistent casing ("Label" vs "label").
    if let Some(json_str) = extract_json_object(&stdout) {
        if let Some(plan) = try_parse_plan(json_str) {
            return Ok(plan);
        }
        // JSON object was found but didn't parse as a plan — might be a wrapper
        if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Could be a stream-json envelope with {"type":"result","result":"..."}
            if let Some(result_str) = envelope.get("result").and_then(|v| v.as_str()) {
                if let Some(inner_json) = extract_json_object(result_str) {
                    if let Some(plan) = try_parse_plan(inner_json) {
                        return Ok(plan);
                    }
                }
            }
            // Result as nested object
            if let Some(result_obj) = envelope.get("result") {
                if result_obj.is_object() {
                    let normalized = normalize_keys(result_obj.clone());
                    if let Ok(plan) = serde_json::from_value::<GeneratedPlan>(normalized) {
                        return Ok(plan);
                    }
                }
            }
        }
    }

    // Strategy 2: Scan stream-json lines for result envelope
    for line in stdout.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let envelope: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if envelope.get("type").and_then(|v| v.as_str()) != Some("result") {
            continue;
        }
        if let Some(result_str) = envelope.get("result").and_then(|v| v.as_str()) {
            if let Some(inner) = extract_json_object(result_str) {
                if let Some(plan) = try_parse_plan(inner) {
                    return Ok(plan);
                }
            }
        }
    }

    // Detect truncation: JSON object started but braces never balanced
    let has_open_brace = stdout.contains('{');
    if has_open_brace && extract_json_object(&stdout).is_none() {
        return Err(
            "The planner response was truncated — the JSON tree is incomplete. \
             Try simplifying the task or breaking it into smaller pieces."
                .to_string(),
        );
    }

    Err(format!(
        "Failed to parse plan from {} output: {}",
        planner_provider_name(provider),
        &stdout[..stdout.len().min(500)]
    ))
}

/// Convert a plan tree into flat DecisionNode records ready for DB insertion.
/// Generates UUIDs and wires parent_id relationships.
pub fn plan_to_nodes(plan: &GeneratedPlan, project_id: &str) -> Vec<DecisionNode> {
    let mut nodes = Vec::new();
    let now = db::now_unix();

    fn visit(
        plan_node: &PlanNode,
        project_id: &str,
        parent_id: Option<String>,
        now: i64,
        nodes: &mut Vec<DecisionNode>,
    ) {
        let id = uuid::Uuid::new_v4().to_string();
        let branch_name = format!("structural/{}/{}", plan_node.node_type, id);

        nodes.push(DecisionNode {
            id: id.clone(),
            project_id: project_id.to_string(),
            parent_id,
            label: plan_node.label.clone(),
            prompt: plan_node.prompt.clone(),
            branch_name,
            worktree_path: None,
            commit_hash: None,
            status: NodeStatus::Pending,
            exit_code: None,
            node_type: Some(plan_node.node_type.clone()),
            scheduled_at: None,
            started_at: None,
            created_at: now,
            updated_at: now,
        });

        for child in &plan_node.children {
            visit(child, project_id, Some(id.clone()), now, nodes);
        }
    }

    visit(&plan.root, project_id, None, now, &mut nodes);
    nodes
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        build_planner_invocation, cleanup_output_file, normalize_plan_for_complexity, GeneratedPlan,
        PlanNode,
    };
    use crate::models::AgentType;

    #[test]
    fn planner_dispatch_builds_claude_invocation() {
        let invocation = build_planner_invocation(
            &AgentType::ClaudeCode,
            "Plan this task",
            Some("sonnet"),
            "/tmp",
        )
        .expect("claude invocation");

        assert_eq!(invocation.program, "claude");
        assert!(invocation.args.iter().any(|arg| arg == "--model"));
        assert!(invocation.output_file.is_none());
    }

    #[test]
    fn planner_dispatch_builds_codex_invocation() {
        let invocation =
            build_planner_invocation(&AgentType::Codex, "Plan this task", Some("gpt-5"), "/tmp")
                .expect("codex invocation");

        assert_eq!(invocation.program, "codex");
        assert_eq!(invocation.args.first().map(String::as_str), Some("exec"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--output-last-message"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--output-schema"));
        assert!(invocation.output_file.is_some());
        cleanup_output_file(invocation.output_file.as_ref());
        cleanup_output_file(invocation.schema_file.as_ref());
    }

    #[test]
    fn planner_dispatch_clamps_reasoning_for_fast_codex_model() {
        let invocation = build_planner_invocation(
            &AgentType::Codex,
            "Plan this task",
            Some("gpt-5-codex-mini"),
            "/tmp",
        )
        .expect("codex invocation");

        assert!(invocation.args.iter().any(|arg| arg == "-c"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "model_reasoning_effort=\"medium\""));
        cleanup_output_file(invocation.output_file.as_ref());
        cleanup_output_file(invocation.schema_file.as_ref());
    }

    #[test]
    fn linear_plan_normalization_flattens_tree_into_chain() {
        let plan = GeneratedPlan {
            root: PlanNode {
                label: "Root".to_string(),
                prompt: "Read the codebase".to_string(),
                node_type: "task".to_string(),
                children: vec![
                    PlanNode {
                        label: "First step".to_string(),
                        prompt: "Do the first thing".to_string(),
                        node_type: "agent".to_string(),
                        children: vec![PlanNode {
                            label: "Nested step".to_string(),
                            prompt: "Do a nested thing".to_string(),
                            node_type: "agent".to_string(),
                            children: vec![],
                        }],
                    },
                    PlanNode {
                        label: "Second step".to_string(),
                        prompt: "Do the second thing".to_string(),
                        node_type: "agent".to_string(),
                        children: vec![],
                    },
                ],
            },
        };

        let normalized = normalize_plan_for_complexity(plan, "linear");

        let mut labels = Vec::new();
        let mut current = &normalized.root;
        while let Some(child) = current.children.first() {
            assert_eq!(current.children.len(), 1);
            labels.push(child.label.clone());
            current = child;
        }

        assert_eq!(labels, vec!["First step", "Nested step"]);
        assert!(normalized.root.children[0].children[0].children.is_empty());
        assert_eq!(normalized.root.node_type, "task");
    }

    #[test]
    fn branching_plan_normalization_is_noop() {
        let plan = GeneratedPlan {
            root: PlanNode {
                label: "Root".to_string(),
                prompt: "Read the codebase".to_string(),
                node_type: "task".to_string(),
                children: vec![
                    PlanNode {
                        label: "Branch A".to_string(),
                        prompt: "First branch".to_string(),
                        node_type: "agent".to_string(),
                        children: vec![],
                    },
                    PlanNode {
                        label: "Branch B".to_string(),
                        prompt: "Second branch".to_string(),
                        node_type: "agent".to_string(),
                        children: vec![],
                    },
                ],
            },
        };

        let normalized = normalize_plan_for_complexity(plan, "branching");

        assert_eq!(normalized.root.children.len(), 2);
        assert_eq!(normalized.root.children[0].label, "Branch A");
        assert_eq!(normalized.root.children[1].label, "Branch B");
    }

    #[test]
    fn cleanup_output_file_removes_temp_file() {
        let file =
            std::env::temp_dir().join(format!("crongen-plan-test-{}.txt", uuid::Uuid::new_v4()));
        fs::write(&file, "plan").expect("write temp file");

        cleanup_output_file(Some(&file));

        assert!(!file.exists());
    }
}
