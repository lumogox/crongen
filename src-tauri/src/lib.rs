mod agent_templates;
mod commands;
mod context;
mod db;
mod git_manager;
mod models;
mod orchestrator;
mod plan_generator;
mod pty_manager;
mod sdk_manager;
mod toon;
#[cfg(windows)]
mod windows_process;

use commands::AppState;
use orchestrator::OrchestratorManager;
use pty_manager::PtyManager;
use rusqlite::Connection;
use sdk_manager::SdkManager;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Logging (debug builds only)
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Initialize SQLite database
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");
            std::fs::create_dir_all(&app_dir).expect("Failed to create app data directory");

            let db_path = app_dir.join("crongen.db");
            log::info!("Database path: {}", db_path.display());

            let conn = Connection::open(&db_path).expect("Failed to open SQLite database");
            db::db_init(&conn).expect("Failed to initialize database schema");

            // Manage app state
            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                pty: Arc::new(PtyManager::new(app_dir.clone())),
                sdk: Arc::new(SdkManager::new(app_dir.clone())),
                orchestrator: Arc::new(OrchestratorManager::new()),
            });

            log::info!("crongen initialized successfully");
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            // Project CRUD
            commands::create_project,
            commands::get_projects,
            commands::get_project,
            commands::update_project,
            commands::delete_project,
            commands::toggle_project,
            // Decision tree
            commands::get_decision_tree,
            commands::run_project_now,
            commands::fork_node,
            commands::create_structural_node,
            commands::create_root_node,
            commands::run_node,
            commands::update_node,
            commands::get_root_nodes,
            commands::merge_node_branch,
            commands::get_merge_preview,
            commands::delete_node_branch,
            // Utility
            commands::check_executable,
            commands::check_env_var,
            commands::get_agent_provider_statuses,
            // PTY
            commands::write_pty,
            commands::resize_pty,
            commands::get_session_output,
            commands::pause_session,
            commands::resume_session,
            // SDK
            commands::get_sdk_session_output,
            // Orchestrator
            commands::start_orchestrator,
            commands::get_orchestrator_status,
            commands::submit_orchestrator_decision,
            commands::cancel_orchestrator,
            // Plan generation
            commands::generate_plan,
            // Settings
            commands::get_settings,
            commands::update_settings,
            commands::validate_node_runtime,
            commands::reset_node_status,
            // Git branch operations
            commands::get_repo_branch,
            commands::create_feature_branch,
            commands::mark_node_merged,
            // Debug
            commands::get_node_context,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
