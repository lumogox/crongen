use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::models::{AutoResponse, ShellExecution};

#[derive(Debug, Clone)]
pub struct ValidationPlan {
    pub label: String,
    pub prompt: String,
    pub execution: ShellExecution,
}

#[derive(Debug, Clone)]
struct ValidationStep {
    description: String,
    command: String,
}

pub fn build_validation_plan(repo_path: &str) -> Result<ValidationPlan, String> {
    let root = Path::new(repo_path);
    let mut steps = Vec::new();

    if let Some(step) = detect_node_step(root)? {
        steps.push(step);
    }
    if let Some(step) = detect_rust_step(root) {
        steps.push(step);
    }
    if let Some(step) = detect_go_step(root) {
        steps.push(step);
    }
    if let Some(step) = detect_dotnet_step(root) {
        steps.push(step);
    }
    if let Some(step) = detect_python_step(root) {
        steps.push(step);
    }
    if let Some(step) = detect_maven_step(root) {
        steps.push(step);
    }
    if let Some(step) = detect_gradle_step(root) {
        steps.push(step);
    }
    if steps.is_empty() {
        if let Some(step) = detect_make_step(root) {
            steps.push(step);
        }
    }

    if steps.is_empty() {
        return Err(
            "No supported validator was detected in the merged repository root.".to_string(),
        );
    }

    let mut script = String::from("set -e\n");
    for step in &steps {
        script.push_str(&format!("echo \"==> {}\"\n", shell_escape_double_quotes(&step.description)));
        script.push_str(&step.command);
        if !step.command.ends_with('\n') {
            script.push('\n');
        }
    }

    let prompt = if steps.len() == 1 {
        format!(
            "Validate the merged main branch by running {} and fail if it exits non-zero.",
            steps[0].description
        )
    } else {
        let descriptions = steps
            .iter()
            .map(|step| step.description.as_str())
            .collect::<Vec<_>>()
            .join(", then ");
        format!(
            "Validate the merged main branch by running {} and fail if any step exits non-zero.",
            descriptions
        )
    };

    Ok(ValidationPlan {
        label: "Validate merged build".to_string(),
        prompt,
        execution: ShellExecution {
            program: "/bin/sh".to_string(),
            args: vec!["-lc".to_string(), script],
            stdin_injection: None,
            auto_responses: Vec::<AutoResponse>::new(),
        },
    })
}

fn detect_node_step(root: &Path) -> Result<Option<ValidationStep>, String> {
    let package_json = root.join("package.json");
    if !package_json.exists() {
        return Ok(None);
    }

    let package_manager = detect_package_manager(root);
    let package = fs::read_to_string(&package_json)
        .map_err(|err| format!("Failed to read {}: {err}", package_json.display()))?;
    let parsed: Value = serde_json::from_str(&package)
        .map_err(|err| format!("Failed to parse {}: {err}", package_json.display()))?;
    let scripts = parsed
        .get("scripts")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();

    let script_name = ["build", "typecheck", "check", "test"]
        .into_iter()
        .find(|name| scripts.contains_key(*name))
        .ok_or_else(|| {
            "package.json is present but no build-like script was found (expected build, typecheck, check, or test)."
                .to_string()
        })?;

    let install_command = if root.join("node_modules").exists() {
        None
    } else {
        Some(match package_manager {
            "bun" => "bun install".to_string(),
            "pnpm" => "pnpm install".to_string(),
            "yarn" => "yarn install".to_string(),
            _ => "npm install".to_string(),
        })
    };

    let run_command = match package_manager {
        "bun" => format!("bun run {script_name}"),
        "pnpm" => format!("pnpm run {script_name}"),
        "yarn" => format!("yarn {script_name}"),
        _ => format!("npm run {script_name}"),
    };

    let mut command = String::new();
    if let Some(install) = install_command {
        command.push_str(&install);
        command.push('\n');
    }
    command.push_str(&run_command);
    command.push('\n');

    Ok(Some(ValidationStep {
        description: format!("{package_manager} {script_name}"),
        command,
    }))
}

fn detect_rust_step(root: &Path) -> Option<ValidationStep> {
    root.join("Cargo.toml")
        .exists()
        .then(|| ValidationStep {
            description: "cargo build".to_string(),
            command: "cargo build\n".to_string(),
        })
}

fn detect_go_step(root: &Path) -> Option<ValidationStep> {
    root.join("go.mod").exists().then(|| ValidationStep {
        description: "go build ./...".to_string(),
        command: "go build ./...\n".to_string(),
    })
}

fn detect_dotnet_step(root: &Path) -> Option<ValidationStep> {
    let target =
        find_first_match(root, &["sln", "csproj"], 4).or_else(|| find_first_named(root, "global.json", 2));

    target.map(|path| {
        let command = match path.extension().and_then(|value| value.to_str()) {
            Some("sln") | Some("csproj") => format!("dotnet build {}\n", shell_quote(&path.display().to_string())),
            _ => "dotnet build\n".to_string(),
        };
        ValidationStep {
            description: "dotnet build".to_string(),
            command,
        }
    })
}

fn detect_python_step(root: &Path) -> Option<ValidationStep> {
    let has_python = root.join("pyproject.toml").exists()
        || root.join("requirements.txt").exists()
        || root.join("setup.py").exists()
        || root.join("setup.cfg").exists();

    if !has_python {
        return None;
    }

    let runner = if root.join("uv.lock").exists() {
        "uv run python"
    } else if root.join("poetry.lock").exists() {
        "poetry run python"
    } else if root.join(".venv").exists() && root.join(".venv/bin/python").exists() {
        "./.venv/bin/python"
    } else {
        "python3"
    };

    let has_tests = root.join("tests").exists()
        || find_first_named(root, "pytest.ini", 2).is_some()
        || find_first_named(root, "conftest.py", 4).is_some()
        || contains_test_file(root, 4);

    let command = if has_tests {
        format!("{runner} -m pytest\n")
    } else {
        format!("{runner} -m compileall .\n")
    };

    Some(ValidationStep {
        description: if has_tests {
            format!("{runner} -m pytest")
        } else {
            format!("{runner} -m compileall .")
        },
        command,
    })
}

fn detect_maven_step(root: &Path) -> Option<ValidationStep> {
    root.join("pom.xml").exists().then(|| ValidationStep {
        description: "mvn -q -DskipTests compile".to_string(),
        command: "mvn -q -DskipTests compile\n".to_string(),
    })
}

fn detect_gradle_step(root: &Path) -> Option<ValidationStep> {
    let has_gradle = root.join("build.gradle").exists()
        || root.join("build.gradle.kts").exists()
        || root.join("settings.gradle").exists()
        || root.join("settings.gradle.kts").exists();
    if !has_gradle {
        return None;
    }

    let command = if root.join("gradlew").exists() {
        "./gradlew build -x test\n".to_string()
    } else {
        "gradle build -x test\n".to_string()
    };

    Some(ValidationStep {
        description: "gradle build -x test".to_string(),
        command,
    })
}

fn detect_make_step(root: &Path) -> Option<ValidationStep> {
    let makefile = ["Makefile", "makefile", "GNUmakefile"]
        .into_iter()
        .find(|name| root.join(name).exists())?;

    let _ = makefile;
    Some(ValidationStep {
        description: "make build".to_string(),
        command: "make build\n".to_string(),
    })
}

fn detect_package_manager(root: &Path) -> &'static str {
    if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        "bun"
    } else if root.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

fn find_first_match(root: &Path, extensions: &[&str], max_depth: usize) -> Option<PathBuf> {
    find_first_with(root, 0, max_depth, &|entry| {
        entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(|ext| extensions.iter().any(|candidate| candidate == &ext))
            .unwrap_or(false)
    })
}

fn find_first_named(root: &Path, file_name: &str, max_depth: usize) -> Option<PathBuf> {
    find_first_with(root, 0, max_depth, &|entry| {
        entry.file_name().to_str() == Some(file_name)
    })
}

fn contains_test_file(root: &Path, max_depth: usize) -> bool {
    find_first_with(root, 0, max_depth, &|entry| {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            return false;
        };
        name.starts_with("test_")
            || name.ends_with("_test.py")
            || name == "test.py"
            || name == "tests.py"
    })
    .is_some()
}

fn find_first_with(
    root: &Path,
    depth: usize,
    max_depth: usize,
    matcher: &dyn Fn(&fs::DirEntry) -> bool,
) -> Option<PathBuf> {
    if depth > max_depth {
        return None;
    }

    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if matcher(&entry) {
            return Some(path);
        }

        if path.is_dir()
            && depth < max_depth
            && !matches!(
                entry.file_name().to_str(),
                Some(".git" | "node_modules" | "target" | ".venv" | "__pycache__")
            )
        {
            if let Some(found) = find_first_with(&path, depth + 1, max_depth, matcher) {
                return Some(found);
            }
        }
    }

    None
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_escape_double_quotes(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::build_validation_plan;

    fn make_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "crongen-validation-test-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn builds_node_validation_from_package_json() {
        let dir = make_temp_dir("node");
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"vite build"}}"#,
        )
        .expect("package.json");

        let plan = build_validation_plan(&dir.display().to_string()).expect("node plan");
        let script = &plan.execution.args[1];
        assert!(script.contains("npm install"));
        assert!(script.contains("npm run build"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn builds_rust_validation_from_cargo_manifest() {
        let dir = make_temp_dir("rust");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("Cargo.toml");

        let plan = build_validation_plan(&dir.display().to_string()).expect("rust plan");
        assert!(plan.execution.args[1].contains("cargo build"));

        let _ = fs::remove_dir_all(dir);
    }
}
