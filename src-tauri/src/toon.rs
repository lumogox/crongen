use crate::context::ExecutionContext;

/// Serialize an ExecutionContext to TOON format for token-efficient delivery
/// to coding agents. Uses the official toon-format crate.
pub fn serialize_execution_context(ctx: &ExecutionContext) -> Result<String, String> {
    let options = toon_format::EncodeOptions::default();
    toon_format::encode(ctx, &options).map_err(|e| format!("Failed to encode TOON: {e}"))
}

/// Wrap TOON context in a clear delimiter block for agent consumption.
pub fn wrap_context_for_prompt(toon: &str) -> String {
    format!("<execution-context>\n{toon}</execution-context>\n\n")
}

/// Convenience: build + serialize + wrap in one call.
/// If the context includes a directive, it is emitted prominently before the TOON block.
pub fn build_context_string(ctx: &ExecutionContext) -> Result<String, String> {
    let toon = serialize_execution_context(ctx)?;
    let mut output = String::new();
    if let Some(ref dir) = ctx.directive {
        output.push_str(&format!(
            "<orchestrator-directive>\n{dir}\n</orchestrator-directive>\n\n"
        ));
    }
    output.push_str(&wrap_context_for_prompt(&toon));
    Ok(output)
}

// Re-export for tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{AncestorStep, SiblingInfo};

    #[test]
    fn test_serialize_basic_context() {
        let ctx = ExecutionContext {
            session_label: "Calculator App".to_string(),
            session_goal: "Create a calculator UI".to_string(),
            ancestor_path: vec![AncestorStep {
                node_type: "task".to_string(),
                label: "Root Task".to_string(),
                prompt: "Create a calculator".to_string(),
                status: "completed".to_string(),
            }],
            current_node: AncestorStep {
                node_type: "agent".to_string(),
                label: "react-impl".to_string(),
                prompt: "Build with React".to_string(),
                status: "pending".to_string(),
            },
            sibling_info: vec![SiblingInfo {
                label: "svelte-impl".to_string(),
                status: "running".to_string(),
                exit_code: None,
            }],
            sibling_diffs: vec![],
            parent_diff: None,
            directive: None,
        };

        let result = serialize_execution_context(&ctx);
        assert!(result.is_ok());
        let toon = result.unwrap();
        assert!(toon.contains("Calculator App"));
        assert!(toon.contains("react-impl"));
    }
}
