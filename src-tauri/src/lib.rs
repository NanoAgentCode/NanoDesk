mod agent_commands;
mod agent_runner;
mod asr;
mod automation;
mod background_agent;
mod brand;
mod cli;
mod code_index;
mod context;
mod context_budget;
mod conversation_service;
mod core;
mod db;
mod error;
mod file_content;
mod llm;
mod logging;
mod mcp;
mod memory;
mod models;
mod observability;
mod ops;
mod plugins;
mod profile;
mod project_files;
mod project_index;
mod project_retrieval;
mod rag;
mod runtime;
mod runtime_events;
mod settings;
mod shell;
mod skills;
mod tool_policy;

mod app_state;
mod commands;
mod services;
mod desktop;

pub(crate) use app_state::{AppState, ChatStreamInterrupts};
pub(crate) use services::observation::{start_observation, finish_observation, ObservationStart};
pub(crate) use services::agent_tools::execute_agent_tool_with_state;

use crate::db::Database;
use crate::desktop::setup_system_tray;
use crate::desktop::show_main_window;
pub(crate) use llm::send_chat_completion;
use tauri::Manager;
use crate::mcp::McpClientManager;
use crate::observability::ObservabilityPipeline;
use crate::observability::SqliteObservabilitySink;
use crate::runtime::RuntimeStore;
use std::collections::HashMap;
use tokio::sync::Mutex;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(err) = show_main_window(app) {
                logging::error(
                    "single-instance",
                    "failed to show main window from second instance",
                    serde_json::json!({ "error": err.to_string() }),
                );
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_system_tray(app)?;

            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| format!("failed to resolve app data directory: {err}"))?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|err| format!("failed to create app data directory: {err}"))?;
            let log_dir = data_dir.join("logs");
            logging::init_system_logger(log_dir)
                .map_err(|err| format!("failed to initialize system logger: {err}"))?;
            logging::info(
                "app",
                format!("{} startup", brand::DISPLAY_NAME),
                serde_json::json!({}),
            );
            logging::debug(
                "app",
                "app data directory resolved",
                serde_json::json!({ "path": data_dir.display().to_string() }),
            );
            let temp_dir = data_dir.join("temp");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|err| format!("failed to create temp directory: {err}"))?;
            let db_path = data_dir.join(brand::MAIN_DATABASE_NAME);
            let db = Database::open(db_path).map_err(|err| err.to_string())?;
            let runtime_path = data_dir.join(brand::RUNTIME_DATABASE_NAME);
            let runtime = RuntimeStore::open(runtime_path).map_err(|err| err.to_string())?;
            let automation = automation::AutomationStore::open(
                &data_dir.join(brand::AUTOMATION_DATABASE_NAME),
            )
            .map_err(|err| err.to_string())?;
            let observability_path = data_dir.join(brand::OBSERVABILITY_DATABASE_NAME);
            let observability = match SqliteObservabilitySink::open(observability_path) {
                Ok(sink) => ObservabilityPipeline::new(vec![Box::new(sink)]),
                Err(err) => {
                    logging::warn(
                        "observability",
                        "observability disabled",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                    ObservabilityPipeline::disabled()
                }
            };

            let plugins = plugins::built_in_registry().map_err(|err| err.to_string())?;

            app.manage(AppState {
                db: Mutex::new(db),
                observability: Mutex::new(observability),
                runtime: Mutex::new(runtime),
                automation: Mutex::new(automation),
                background_agents: background_agent::BackgroundAgentManager::default(),
                mcp: Mutex::new(McpClientManager::default()),
                plugins,
                ops_ssh_sessions: Mutex::new(HashMap::new()),
                chat_stream_interrupts: Mutex::new(ChatStreamInterrupts::default()),
            });
            profile::start_worker(app.handle().clone());
            automation::start_worker(app.handle().clone());
            background_agent::restore_waiting_runs(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            background_agent::commands::start_background_agent,
            background_agent::commands::list_background_agents,
            background_agent::commands::respond_background_agent,
            background_agent::commands::stop_background_agent,
            automation::commands::list_automations,
            automation::commands::save_automation,
            automation::commands::set_automation_enabled,
            automation::commands::delete_automation,
            automation::commands::list_automation_runs,
            automation::commands::run_automation_now,
            automation::commands::recover_automation_run,
            commands::items::list_items,
            commands::items::search_items,
            commands::items::create_item,
            commands::items::update_item,
            commands::items::delete_item,
            commands::models::list_model_configs,
            commands::models::list_model_suppliers,
            commands::models::save_model_supplier,
            commands::models::delete_model_supplier,
            commands::models::save_model_config,
            commands::models::delete_model_config,
            commands::mcp::list_mcp_servers,
            commands::mcp::restore_mcp_servers,
            commands::mcp::save_mcp_server,
            commands::mcp::delete_mcp_server,
            commands::mcp::connect_mcp_server,
            commands::mcp::disconnect_mcp_server,
            commands::mcp::refresh_mcp_tools,
            commands::mcp::call_mcp_tool,
            ops::list_ops_servers,
            ops::save_ops_server,
            ops::delete_ops_server,
            ops::test_ops_ssh_connection,
            ops::upload_ops_file,
            ops::start_ops_ssh_session,
            ops::send_ops_ssh_input,
            ops::resize_ops_ssh_session,
            ops::stop_ops_ssh_session,
            ops::ask_ops_ai,
            commands::models::test_llm_connectivity,
            commands::models::list_available_models,
            commands::models::test_embedding_connectivity,
            commands::conversations::list_conversations,
            commands::conversations::list_archived_conversations,
            commands::conversations::list_conversation_project_paths,
            commands::conversations::create_conversation,
            commands::conversations::delete_conversation,
            commands::conversations::archive_conversation,
            commands::conversations::rename_conversation,
            commands::conversations::update_conversation_model,
            commands::conversations::list_messages,
            commands::conversations::append_message,
            commands::conversations::delete_messages,
            rag::list_rag_files,
            rag::index_rag_file,
            rag::delete_rag_file,
            rag::search_rag_context,
            code_index::index_project_code,
            code_index::get_code_index_stats,
            code_index::search_code_index,
            project_index::index_project_documents,
            project_index::get_project_index_stats,
            project_index::search_project_index,
            context::load_base_context,
            context_budget::plan_context_preparation,
            context_budget::fit_context_messages,
            memory::list_memories,
            memory::list_enabled_memories,
            profile::get_user_profile,
            profile::get_profile_context,
            profile::get_profile_settings,
            profile::save_profile_settings,
            profile::get_profile_processing_status,
            profile::list_filtered_profile_observations,
            profile::include_filtered_profile_observation,
            profile::discard_filtered_profile_observation,
            profile::delete_profile_fact,
            profile::clear_user_profile,
            profile::retry_profile_failures,
            profile::run_profile_worker_now,
            profile::generate_profile_now,
            memory::list_relevant_memories,
            memory::search_memories,
            memory::create_memory,
            memory::update_memory,
            memory::delete_memory,
            commands::skills::sync_anthropic_skills,
            commands::skills::sync_github_skills,
            commands::skills::list_local_skills,
            settings::get_tavily_api_key,
            settings::save_tavily_api_key,
            settings::get_asr_config,
            settings::save_asr_config,
            asr::transcribe_audio,
            asr::transcribe_audio_file,
            commands::chat::chat,
            commands::chat::chat_stream,
            commands::chat::interrupt_chat_stream,
            agent_commands::create_agent_run,
            agent_commands::finish_agent_run,
            agent_commands::resume_agent_run,
            agent_commands::list_agent_runs,
            agent_commands::list_agent_run_timelines,
            agent_commands::list_agent_event_logs,
            agent_commands::retry_agent_tool_call,
            agent_commands::record_agent_step,
            agent_commands::create_agent_tool_call,
            agent_commands::update_agent_tool_call,
            agent_commands::approve_agent_tool_call,
            agent_commands::resolve_agent_tool_approval,
            agent_commands::reject_agent_tool_call,
            agent_commands::list_agent_tool_definitions,
            agent_commands::list_plugins,
            agent_commands::resolve_agent_model_output,
            commands::agent_tools::execute_agent_tool_call,
            commands::environment::check_env,
            commands::environment::install_env,
            project_files::is_directory_empty,
            project_files::list_project_files,
            project_files::read_project_file,
            project_files::create_project_file,
            project_files::write_project_file,
            project_files::delete_project_file,
            project_files::rename_project_file,
            commands::images::save_chat_image_attachment,
            commands::images::read_chat_image_attachment,
            project_files::open_project_file_location,
            project_files::open_project_location,
            commands::project_tools::execute_bash_command,
            commands::project_tools::write_local_file,
            commands::project_tools::read_local_file,
            file_content::read_absolute_file,
            file_content::extract_uploaded_file,
            commands::observability::list_observability_spans,
            commands::observability::clear_observability_spans,
            commands::observability::get_usage_analysis,
            desktop::show_app_window,
            desktop::minimize_to_tray,
            desktop::quit_app,
            desktop::get_autostart,
            desktop::set_autostart
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("error while running {}: {error}", brand::DISPLAY_NAME));
}

pub fn run_cli() -> i32 {
    cli::run()
}

#[cfg(test)]
mod chat_stream_interrupt_tests {
    use super::ChatStreamInterrupts;

    #[test]
    fn registered_stream_can_be_interrupted_and_removed() {
        let mut interrupts = ChatStreamInterrupts::default();
        let mut receiver = interrupts.register("request-1");

        assert!(interrupts.interrupt("request-1"));
        assert!(receiver.has_changed().unwrap());
        assert!(*receiver.borrow_and_update());

        interrupts.remove("request-1");
        assert!(!interrupts.interrupt("request-1"));
    }
}
