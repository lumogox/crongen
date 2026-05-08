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
    pub node_type: String, // "task", "decision", "agent", "merge", "synthesis", "final", "validation"
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
- "decision" nodes contain 2+ "agent" children PLUS a "merge" or "synthesis" child (last)
- "merge" nodes compare alternatives and pick one winner. Use "synthesis" when complementary ideas should be combined.
- "merge" and "synthesis" nodes contain one "final" child
- The tree is NESTED: task > decision > [agents..., merge/synthesis > final]
- Max 8 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo System", "Auth Login Page"), NOT the structural role. Child labels describe what each step does specifically.

CRITICAL prompt scoping rules:
- The root "task" prompt ONLY handles scaffolding: project init, install dependencies, create config files, set up folder structure. It must NOT make design choices, pick approaches, or implement features.
- Agent prompts under a "decision" MUST say "Implement/Apply X to the existing project" — they modify the codebase left by the root task, they do NOT scaffold or create a new project.
- The "merge" prompt evaluates branches and chooses the single best branch. The "synthesis" prompt combines useful parts from multiple branches into a better result.
- The "final" prompt applies finishing touches to the chosen result — it does NOT re-scaffold.

Example for user task "Build a calculator with theme support":
{"root":{"label":"Calculator Themes","prompt":"Scaffold a new Vite + TypeScript project, install dependencies, and create the basic folder structure. Do NOT implement any features or make design choices.","node_type":"task","children":[{"label":"Theme Strategy","prompt":"Pick theme implementation strategy","node_type":"decision","children":[{"label":"CSS Variables","prompt":"Implement theming using CSS custom properties in the existing project","node_type":"agent","children":[]},{"label":"Tailwind Dark Mode","prompt":"Implement theming using Tailwind dark mode classes in the existing project","node_type":"agent","children":[]},{"label":"Evaluate Themes","prompt":"Evaluate both theme approaches","node_type":"merge","children":[{"label":"Polish UI","prompt":"Polish and finalize the chosen theme approach","node_type":"final","children":[]}]}]}]}}"#;

const PLAN_SYSTEM_PROMPT_EXISTING: &str = r#"You are a task decomposition planner for an EXISTING codebase. Output ONLY raw JSON, no markdown fences, no explanation.

Rules:
- Root node: type "task" (one child: a "decision" or "agent")
- "decision" nodes contain 2+ "agent" children PLUS a "merge" or "synthesis" child (last)
- "merge" nodes compare alternatives and pick one winner. Use "synthesis" when complementary ideas should be combined.
- "merge" and "synthesis" nodes contain one "final" child
- The tree is NESTED: task > decision > [agents..., merge/synthesis > final]
- Max 8 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo System", "Dark Mode Toggle"), NOT the structural role. Child labels describe what each step does specifically.

CRITICAL prompt scoping rules for EXISTING projects:
- The root "task" prompt reads and analyzes the existing codebase — it does NOT scaffold, create a new project, or re-initialize anything. It should understand the project structure, conventions, and key files before implementation begins.
- Agent prompts under a "decision" MUST build on the existing code. They implement the requested feature/change within the existing architecture and conventions.
- The "merge" prompt evaluates branches and chooses the single best branch. The "synthesis" prompt combines useful parts from multiple branches into a better result.
- The "final" prompt integrates the chosen approach, updates tests, and ensures consistency with the rest of the codebase.

Example for user task "Add undo/redo to the calculator":
{"root":{"label":"Undo/Redo System","prompt":"Read the existing project structure, understand the architecture and state management. Do NOT create new projects or re-scaffold.","node_type":"task","children":[{"label":"Undo Strategy","prompt":"Pick undo/redo implementation strategy","node_type":"decision","children":[{"label":"History Stack","prompt":"Implement undo/redo using a history stack array in the existing codebase","node_type":"agent","children":[]},{"label":"Command Pattern","prompt":"Implement undo/redo using the command pattern in the existing codebase","node_type":"agent","children":[]},{"label":"Compare Approaches","prompt":"Evaluate both undo/redo implementations","node_type":"merge","children":[{"label":"Finalize Undo/Redo","prompt":"Integrate the chosen approach, add keyboard shortcuts, and test","node_type":"final","children":[]}]}]}]}}"#;

const PLAN_LINEAR_PROMPT: &str = r#"You are a task planner. Output ONLY raw JSON, no markdown fences, no explanation.

Rules for LINEAR plans (no branching, no decisions):
- Root node: type "task" — sets up the project
- The tree is a single chain of "agent" nodes: task > agent > agent > ...
- Each node has at most one child
- NO "decision", "merge", "synthesis", or "final" nodes
- Max 3 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Auth System", "Search Feature"), NOT the structural role. Child labels describe specific steps.

Example for user task "Build a todo list app":
{"root":{"label":"Todo List App","prompt":"Scaffold a new Vite + React project with Tailwind CSS.","node_type":"task","children":[{"label":"Build Todo UI","prompt":"Implement the todo list with add, complete, and delete functionality.","node_type":"agent","children":[{"label":"Polish Todo UI","prompt":"Refine the interface and add basic validation.","node_type":"agent","children":[]}]}]}}"#;

const PLAN_LINEAR_EXISTING_PROMPT: &str = r#"You are a task planner for an EXISTING codebase. Output ONLY raw JSON, no markdown fences, no explanation.

Rules for LINEAR plans (no branching, no decisions):
- Root node: type "task" — reads and understands the existing codebase (do NOT scaffold or re-init)
- The tree is a single chain of "agent" nodes: task > agent > agent > ...
- Each node has at most one child
- NO "decision", "merge", "synthesis", or "final" nodes
- Max 3 total nodes. Prompts: 1-2 sentences.

LABELING: The root label MUST summarize the user's task (e.g. "Undo/Redo", "Dark Mode"), NOT the structural role. Child labels describe specific steps.

Example for user task "Add undo/redo to the calculator":
{"root":{"label":"Undo/Redo","prompt":"Read the project structure and understand how state is managed. Do NOT scaffold.","node_type":"task","children":[{"label":"Implement Undo/Redo","prompt":"Add undo/redo using a history stack, with Ctrl+Z/Ctrl+Y keyboard shortcuts.","node_type":"agent","children":[{"label":"Polish Undo/Redo","prompt":"Add tests and verify the implementation fits the existing codebase.","node_type":"agent","children":[]}]}]}}"#;

const REFINE_SYSTEM_PROMPT: &str = r#"You are an orchestration plan editor. Output ONLY raw JSON, no markdown fences, no explanation.

You will receive the current crongen execution flow and refinement guidance.
Return a better flow using this exact schema:
{"root":{"label":"...","prompt":"...","node_type":"task","children":[]}}

Allowed node types:
- "task": root task only. Summarizes the user goal and sets the starting context.
- "agent": executable work step. This is a work item, not a provider name.
- "decision": structural branch point. Use when alternatives should be explored.
- "merge": executable compare step. Evaluates sibling branches and chooses the single best result.
- "synthesis": executable synthesis step. Combines useful parts from sibling branches into one improved result.
- "final": executable finish/polish step after comparison or implementation.
- "validation": executable local validation/check step.

Rules:
- Preserve the root as node_type "task".
- Keep labels short and action-oriented.
- Make prompts specific enough for an execution agent to act without guessing.
- You may rewrite, add, remove, reorder, or restructure nodes when the guidance asks for it.
- Preserve useful intent from the current flow unless the guidance explicitly steers elsewhere.
- For decision nodes, include at least 2 agent children. Add a merge child for mutually exclusive alternatives, or a synthesis child when branches may contribute complementary pieces.
- Put validation after implementation, compare, or finish steps when checks matter.
- Do not mention Claude, Codex, Gemini, or provider-specific details inside node labels/prompts unless the user explicitly asks.
- For existing projects, do not scaffold or re-initialize; prompts should build on the current codebase.
- Max 10 total nodes. Prompts: 1-3 concise sentences.
"#;

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

fn codex_model_requires_default_retry(details: &str) -> bool {
    let normalized = details.to_lowercase();
    normalized.contains("requires a newer version of codex")
        || normalized.contains("model requires a newer version")
        || (normalized.contains("unsupported model") && normalized.contains("codex"))
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
                        "enum": ["task", "decision", "agent", "merge", "synthesis", "final", "validation"]
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

fn plan_path_count_guidance(complexity: &str, path_count: u32) -> String {
    if complexity == "linear" {
        return "Path count: 1. Generate one direct execution chain with no decision, merge, synthesis, or final nodes.".to_string();
    }

    let path_count = path_count.clamp(1, 10);
    if path_count == 1 {
        return "Path count: 1. Generate one direct implementation path: root task with one agent child, optionally followed by validation. Do NOT create a decision, merge, or synthesis node because there are no alternatives to compare.".to_string();
    }

    format!(
        "Path count: {path_count}. This overrides any generic examples above. Generate exactly {path_count} alternative agent children under the decision node before the merge or synthesis node. Do not generate fewer or more alternative paths. The total node limit is {max_nodes} nodes.",
        max_nodes = path_count + 4
    )
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
    extra_args: &[String],
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
            append_extra_args(&mut args, extra_args);

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
                "--ignore-user-config".to_string(),
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
            append_extra_args(&mut args, extra_args);
            args.push(full_prompt.to_string());

            Ok(PlannerInvocation {
                program: "codex".to_string(),
                args,
                current_dir: PathBuf::from(repo_path),
                output_file: Some(output_file),
                schema_file: Some(schema_file),
            })
        }
        AgentType::Gemini => {
            let mut args = Vec::new();
            args.push("--approval-mode".to_string());
            args.push("plan".to_string());
            args.push("--output-format".to_string());
            args.push("json".to_string());
            if let Some(m) = model {
                args.push("--model".to_string());
                args.push(m.to_string());
            }
            append_extra_args(&mut args, extra_args);
            args.push("--prompt".to_string());
            args.push(full_prompt.to_string());

            Ok(PlannerInvocation {
                program: "gemini".to_string(),
                args,
                current_dir: PathBuf::from(repo_path),
                output_file: None,
                schema_file: None,
            })
        }
        AgentType::Custom => Err("Custom providers are not supported for planning.".to_string()),
    }
}

fn append_extra_args(args: &mut Vec<String>, extra_args: &[String]) {
    args.extend(
        extra_args
            .iter()
            .map(|arg| arg.trim())
            .filter(|arg| !arg.is_empty())
            .map(ToString::to_string),
    );
}

async fn run_planner_invocation(
    provider: &AgentType,
    invocation: PlannerInvocation,
) -> Result<String, String> {
    let output_file = invocation.output_file.clone();
    let schema_file = invocation.schema_file.clone();

    let output = Command::new(&invocation.program)
        .args(&invocation.args)
        .current_dir(&invocation.current_dir)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn {}: {e}", invocation.program))?;

    if !output.status.success() {
        cleanup_output_file(output_file.as_ref());
        cleanup_output_file(schema_file.as_ref());
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

    let raw_output = if let Some(output_file) = &output_file {
        match fs::read_to_string(output_file) {
            Ok(contents) => contents,
            Err(err) => {
                cleanup_output_file(Some(output_file));
                cleanup_output_file(schema_file.as_ref());
                return Err(format!("Failed to read planner output: {err}"));
            }
        }
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    cleanup_output_file(output_file.as_ref());
    cleanup_output_file(schema_file.as_ref());

    Ok(raw_output)
}

async fn run_planner(
    provider: &AgentType,
    full_prompt: &str,
    model: Option<&str>,
    extra_args: &[String],
    repo_path: &str,
) -> Result<String, String> {
    let invocation = build_planner_invocation(provider, full_prompt, model, extra_args, repo_path)?;
    let result = run_planner_invocation(provider, invocation).await;

    if let Err(err) = &result {
        if matches!(provider, AgentType::Codex)
            && model.is_some()
            && codex_model_requires_default_retry(err)
        {
            if let Some(model) = model {
                log::warn!(
                    "Codex planner model '{}' is unsupported by the installed Codex CLI; retrying with the Codex default model",
                    model
                );
            }
            let fallback_invocation =
                build_planner_invocation(provider, full_prompt, None, extra_args, repo_path)?;
            return run_planner_invocation(provider, fallback_invocation).await;
        }
    }

    result
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
    attached_context: Option<&str>,
    project_mode: &str,
    model: Option<&str>,
    extra_args: &[String],
    complexity: &str,
    path_count: u32,
    repo_path: &str,
) -> Result<GeneratedPlan, String> {
    let system_prompt = planner_system_prompt(project_mode, complexity);
    let path_guidance = plan_path_count_guidance(complexity, path_count);
    let full_prompt = match attached_context.filter(|value| !value.trim().is_empty()) {
        Some(context) => {
            format!("{system_prompt}\n\n{path_guidance}\n\n{context}\n\nUser task: {prompt}")
        }
        None => format!("{system_prompt}\n\n{path_guidance}\n\nUser task: {prompt}"),
    };
    let raw_output = run_planner(provider, &full_prompt, model, extra_args, repo_path).await?;
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
            // Gemini JSON output wraps the final text in a `response` field.
            if let Some(response_str) = envelope.get("response").and_then(|v| v.as_str()) {
                if let Some(inner_json) = extract_json_object(response_str) {
                    if let Some(plan) = try_parse_plan(inner_json) {
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

pub async fn refine_plan(
    provider: &AgentType,
    current_nodes: &[DecisionNode],
    attached_context: Option<&str>,
    project_mode: &str,
    lenses: &[String],
    guidance: Option<&str>,
    model: Option<&str>,
    extra_args: &[String],
    repo_path: &str,
) -> Result<GeneratedPlan, String> {
    let current_flow_json = serde_json::to_string_pretty(current_nodes)
        .map_err(|err| format!("Failed to serialize current flow: {err}"))?;
    let lens_text = if lenses.is_empty() {
        "Baseline polish: clarify labels, tighten prompts, improve flow coherence.".to_string()
    } else {
        lenses.join(", ")
    };
    let guidance_text = guidance
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No additional guidance.");
    let mode_guidance = if project_mode == "existing" {
        "This is an existing codebase. Do not scaffold or re-initialize the project."
    } else {
        "This may be a blank project. Keep any needed setup scoped to the root task."
    };

    let full_prompt = match attached_context.filter(|value| !value.trim().is_empty()) {
        Some(context) => format!(
            "{REFINE_SYSTEM_PROMPT}\n\nProject mode: {project_mode}\n{mode_guidance}\n\n{context}\n\nRefinement lenses: {lens_text}\n\nUser guidance:\n{guidance_text}\n\nCurrent flow JSON:\n{current_flow_json}"
        ),
        None => format!(
            "{REFINE_SYSTEM_PROMPT}\n\nProject mode: {project_mode}\n{mode_guidance}\n\nRefinement lenses: {lens_text}\n\nUser guidance:\n{guidance_text}\n\nCurrent flow JSON:\n{current_flow_json}"
        ),
    };
    let raw_output = run_planner(provider, &full_prompt, model, extra_args, repo_path).await?;

    if let Some(json_str) = extract_json_object(&raw_output) {
        if let Some(plan) = try_parse_plan(json_str) {
            return Ok(plan);
        }
        if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(json_str) {
            if let Some(result_str) = envelope.get("result").and_then(|v| v.as_str()) {
                if let Some(inner_json) = extract_json_object(result_str) {
                    if let Some(plan) = try_parse_plan(inner_json) {
                        return Ok(plan);
                    }
                }
            }
            if let Some(result_obj) = envelope.get("result") {
                if result_obj.is_object() {
                    let normalized = normalize_keys(result_obj.clone());
                    if let Ok(plan) = serde_json::from_value::<GeneratedPlan>(normalized) {
                        return Ok(plan);
                    }
                }
            }
            if let Some(response_str) = envelope.get("response").and_then(|v| v.as_str()) {
                if let Some(inner_json) = extract_json_object(response_str) {
                    if let Some(plan) = try_parse_plan(inner_json) {
                        return Ok(plan);
                    }
                }
            }
        }
    }

    for line in raw_output.lines().rev() {
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

    Err(format!(
        "Failed to parse refined plan from {} output: {}",
        planner_provider_name(provider),
        &raw_output[..raw_output.len().min(500)]
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
            agent_type_override: None,
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

pub fn plan_children_to_nodes(
    plan: &GeneratedPlan,
    project_id: &str,
    parent_id: &str,
) -> Vec<DecisionNode> {
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
            agent_type_override: None,
            scheduled_at: None,
            started_at: None,
            created_at: now,
            updated_at: now,
        });

        for child in &plan_node.children {
            visit(child, project_id, Some(id.clone()), now, nodes);
        }
    }

    for child in &plan.root.children {
        visit(
            child,
            project_id,
            Some(parent_id.to_string()),
            now,
            &mut nodes,
        );
    }

    nodes
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        build_planner_invocation, cleanup_output_file, codex_model_requires_default_retry,
        normalize_plan_for_complexity, plan_children_to_nodes, plan_path_count_guidance,
        write_plan_output_schema, GeneratedPlan, PlanNode,
    };
    use crate::models::AgentType;

    #[test]
    fn planner_dispatch_builds_claude_invocation() {
        let invocation = build_planner_invocation(
            &AgentType::ClaudeCode,
            "Plan this task",
            Some("sonnet"),
            &[],
            "/tmp",
        )
        .expect("claude invocation");

        assert_eq!(invocation.program, "claude");
        assert!(invocation.args.iter().any(|arg| arg == "--model"));
        assert!(invocation.output_file.is_none());
    }

    #[test]
    fn plan_schema_accepts_synthesis_nodes() {
        let schema_file = write_plan_output_schema().expect("schema file");
        let schema = fs::read_to_string(&schema_file).expect("schema content");

        assert!(schema.contains("\"synthesis\""));
        cleanup_output_file(Some(&schema_file));
    }

    #[test]
    fn path_count_guidance_controls_branch_width() {
        let guidance = plan_path_count_guidance("branching", 4);
        assert!(guidance.contains("exactly 4 alternative agent children"));
        assert!(guidance.contains("total node limit is 8 nodes"));

        let single_path = plan_path_count_guidance("branching", 1);
        assert!(single_path.contains("Do NOT create a decision"));

        let clamped = plan_path_count_guidance("branching", 99);
        assert!(clamped.contains("Path count: 10"));
    }

    #[test]
    fn planner_dispatch_builds_codex_invocation() {
        let invocation = build_planner_invocation(
            &AgentType::Codex,
            "Plan this task",
            Some("gpt-5"),
            &[],
            "/tmp",
        )
        .expect("codex invocation");

        assert_eq!(invocation.program, "codex");
        assert_eq!(invocation.args.first().map(String::as_str), Some("exec"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--ignore-user-config"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--output-last-message"));
        assert!(invocation.args.iter().any(|arg| arg == "--output-schema"));
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
            &[],
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
    fn codex_model_requires_default_retry_detects_newer_cli_error() {
        let details = "ERROR: {\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'gpt-5.5' model requires a newer version of Codex. Please upgrade to the latest app or CLI and try again.\"}}";

        assert!(codex_model_requires_default_retry(details));
        assert!(!codex_model_requires_default_retry(
            "Codex exited with error: authentication failed"
        ));
    }

    #[test]
    fn planner_dispatch_builds_gemini_invocation() {
        let invocation = build_planner_invocation(
            &AgentType::Gemini,
            "Plan this task",
            Some("gemini-3-pro"),
            &["--include-directories".to_string(), "../shared".to_string()],
            "/tmp",
        )
        .expect("gemini invocation");

        assert_eq!(invocation.program, "gemini");
        assert!(invocation.args.iter().any(|arg| arg == "--prompt"));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "plan"]));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--include-directories", "../shared"]));
        assert!(invocation.args.iter().any(|arg| arg == "--output-format"));
        assert!(invocation.args.iter().any(|arg| arg == "json"));
        assert!(invocation.output_file.is_none());
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
    fn plan_children_to_nodes_attaches_generated_children_to_parent() {
        let plan = GeneratedPlan {
            root: PlanNode {
                label: "Generated root".to_string(),
                prompt: "Root prompt".to_string(),
                node_type: "task".to_string(),
                children: vec![PlanNode {
                    label: "Generated child".to_string(),
                    prompt: "Child prompt".to_string(),
                    node_type: "agent".to_string(),
                    children: vec![PlanNode {
                        label: "Generated grandchild".to_string(),
                        prompt: "Grandchild prompt".to_string(),
                        node_type: "agent".to_string(),
                        children: vec![],
                    }],
                }],
            },
        };

        let nodes = plan_children_to_nodes(&plan, "project-1", "parent-1");

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].label, "Generated child");
        assert_eq!(nodes[0].parent_id.as_deref(), Some("parent-1"));
        assert_eq!(nodes[1].label, "Generated grandchild");
        assert_eq!(nodes[1].parent_id.as_deref(), Some(nodes[0].id.as_str()));
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
