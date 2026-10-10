mod theme;
mod models;
mod context;
mod presentation;
use theme::CliTheme;
use models::*;
use context::prepare_cli_context;
use presentation::*;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::thread;

use uuid::Uuid;

use crate::brand;
use crate::code_index::{persist_project_code_index, prepare_project_code_index};
use crate::context::load_base_context_for_db;
use crate::context_budget::{
    build_summary_prompt, fit_context_messages, prepare_context_plan, SUMMARY_OUTPUT_TOKENS,
};
use crate::conversation_service::{
    append_conversation_message, bind_conversation_model, create_conversation,
    load_conversation_history as service_load_conversation_history,
};
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::llm::{send_chat_completion, stream_chat_completion};
use crate::models::{
    ChatMessage, ChatRequest, ChatStreamEvent, ChatStreamRequest, ContextPreparationRequest,
    ContextSummaryMetadata, Conversation, ConversationDraft, Message, MessageDraft,
    MessageMetadata, ModelConfig, ModelConfigDraft, ProjectFileEntry,
};
use crate::project_files::{list_project_files, project_root};
#[cfg(test)]
use crate::project_index::DOCUMENT_INDEXER;
use crate::project_index::{persist_project_document_index, prepare_project_document_index};

const EMBEDDING_CONFIG_ID: &str = "embedding-config";

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionMode {
    Project(PathBuf),
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    mode: SessionMode,
    prompt: Option<String>,
    model: Option<String>,
    data_dir: Option<PathBuf>,
    rebuild_index: bool,
    continue_latest: bool,
    resume: Option<String>,
    list_sessions: bool,
    show_session: Option<String>,
    list_files: bool,
    help: bool,
    version: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            mode: SessionMode::Project(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            prompt: None,
            model: None,
            data_dir: None,
            rebuild_index: true,
            continue_latest: false,
            resume: None,
            list_sessions: false,
            show_session: None,
            list_files: false,
            help: false,
            version: false,
        }
    }
}

pub fn run() -> i32 {
    let options = match parse_args(env::args().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            print_error(&err);
            eprintln!();
            print_help();
            return 2;
        }
    };

    if options.help {
        print_help();
        return 0;
    }
    if options.version {
        println!("nano {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            print_error(&format!("无法启动异步运行时: {err}"));
            return 1;
        }
    };

    match runtime.block_on(run_session(options)) {
        Ok(()) => 0,
        Err(err) => {
            print_error(&err.to_string());
            1
        }
    }
}

fn parse_args<I>(args: I) -> Result<CliOptions, String>
where
    I: IntoIterator<Item = String>,
{
    let mut options = CliOptions::default();
    let mut args = args.into_iter();
    let mut mode_was_set = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => options.help = true,
            "-V" | "--version" => options.version = true,
            "--temp" => {
                if mode_was_set {
                    return Err("--temp 与 --project 不能同时使用".to_string());
                }
                options.mode = SessionMode::Temporary;
                mode_was_set = true;
            }
            "--project" | "-C" => {
                if mode_was_set {
                    return Err("--project 与 --temp 不能同时使用".to_string());
                }
                options.mode = SessionMode::Project(PathBuf::from(next_value(&mut args, &arg)?));
                mode_was_set = true;
            }
            "-p" | "--prompt" => options.prompt = Some(next_value(&mut args, &arg)?),
            "-m" | "--model" => options.model = Some(next_value(&mut args, &arg)?),
            "--data-dir" => options.data_dir = Some(PathBuf::from(next_value(&mut args, &arg)?)),
            "--no-index" => options.rebuild_index = false,
            "--continue" => options.continue_latest = true,
            "--resume" => options.resume = Some(next_value(&mut args, &arg)?),
            "--sessions" => options.list_sessions = true,
            "--show" => options.show_session = Some(next_value(&mut args, &arg)?),
            "--files" => options.list_files = true,
            _ if arg.starts_with('-') => return Err(format!("未知参数: {arg}")),
            _ => {
                if options.prompt.is_some() {
                    return Err(format!("多余的位置参数: {arg}"));
                }
                options.prompt = Some(arg);
            }
        }
    }

    if matches!(options.mode, SessionMode::Temporary)
        && (options.continue_latest
            || options.resume.is_some()
            || options.list_sessions
            || options.show_session.is_some()
            || options.list_files)
    {
        return Err("--temp 不支持项目会话或项目文件操作".to_string());
    }
    if options.continue_latest && options.resume.is_some() {
        return Err("--continue 与 --resume 不能同时使用".to_string());
    }
    let inspection_count = usize::from(options.list_sessions)
        + usize::from(options.show_session.is_some())
        + usize::from(options.list_files);
    if inspection_count > 1 {
        return Err("--sessions、--show 与 --files 不能同时使用".to_string());
    }
    if inspection_count > 0
        && (options.continue_latest || options.resume.is_some() || options.prompt.is_some())
    {
        return Err("查看或列表参数不能与恢复参数或问题同时使用".to_string());
    }

    Ok(options)
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{option} 缺少参数值"))
}

async fn run_session(options: CliOptions) -> AppResult<()> {
    let data_dir = match options.data_dir.clone() {
        Some(path) => path,
        None => default_app_data_dir()?,
    };
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join(brand::MAIN_DATABASE_NAME);
    let db = Database::open(db_path.clone())?;
    let project = match &options.mode {
        SessionMode::Temporary => None,
        SessionMode::Project(path) => {
            let root = project_root(&path.to_string_lossy())?;
            Some(root)
        }
    };
    let project_session_path = project.as_deref().map(display_project_path);

    if options.list_sessions {
        let sessions = db.list_conversations(project_session_path.as_deref())?;
        print_sessions(&sessions);
        return Ok(());
    }
    if let Some(selector) = options.show_session.as_deref() {
        let conversation = resolve_requested_conversation(
            &db,
            project_session_path.as_deref(),
            false,
            Some(selector),
        )?
        .ok_or_else(|| AppError::Message("未找到指定会话".to_string()))?;
        let messages = db.list_messages(&conversation.id)?;
        print_conversation(&conversation, &messages);
        return Ok(());
    }
    if options.list_files {
        let root = project
            .as_deref()
            .ok_or_else(|| AppError::Message("--files 需要项目目录".to_string()))?;
        let files = list_project_files(root.to_string_lossy().to_string()).await?;
        print_project_files(root, &files);
        return Ok(());
    }

    let resumed_conversation = resolve_requested_conversation(
        &db,
        project_session_path.as_deref(),
        options.continue_latest,
        options.resume.as_deref(),
    )?;
    let mut conversation_id = resumed_conversation
        .as_ref()
        .map(|conversation| conversation.id.clone());
    let mut history = match conversation_id.as_deref() {
        Some(id) => load_conversation_history(&db, id)?,
        None => Vec::new(),
    };

    let mut models = ensure_chat_models(&db, configure_initial_model)?;
    let saved_model_id = resumed_conversation
        .as_ref()
        .and_then(|conversation| conversation.model_config_id.as_deref());
    let mut active_model =
        resolve_session_model(&models, options.model.as_deref(), saved_model_id)?;
    if let Some(id) = conversation_id.as_deref() {
        if options.model.is_some() || saved_model_id != Some(active_model.id.as_str()) {
            bind_conversation_model(&db, id, Some(&active_model.id))?;
        }
    }

    if let Some(root) = project.as_deref() {
        if options.rebuild_index {
            rebuild_project_indexes(&db, root).await?;
        }
    }
    start_profile_worker(db_path);

    if let Some(prompt) = options.prompt.as_deref() {
        ask(
            &db,
            &active_model,
            project.as_deref(),
            project_session_path.as_deref(),
            &mut conversation_id,
            &mut history,
            prompt,
        )
        .await?;
        if let Err(error) = crate::profile::run_database_worker_cycle(&db).await {
            crate::logging::warn(
                "profile-cli-worker",
                "post-response cycle failed",
                serde_json::json!({ "error": error.to_string() }),
            );
        }
        return Ok(());
    }

    print_banner(
        &active_model,
        project.as_deref(),
        resumed_conversation.as_ref(),
    );
    loop {
        print!("{} ", CliTheme::stdout().prompt("nano>"));
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }
        if input == "/exit" || input == "/quit" {
            break;
        }
        if input == "/help" {
            print_interactive_help();
            continue;
        }
        if input == "/clear" {
            history.clear();
            if project.is_some() {
                conversation_id = None;
                print_success("已结束当前会话，下一条消息将创建新项目会话。");
            } else {
                print_success("已清空当前临时上下文。");
            }
            continue;
        }
        if input == "/model" {
            print_models(&models, &active_model);
            continue;
        }
        if input == "/model add" {
            match add_chat_model(&db, &mut models, configure_additional_model) {
                Ok(model) => {
                    active_model = model;
                    if let Some(id) = conversation_id.as_deref() {
                        bind_conversation_model(&db, id, Some(&active_model.id))?;
                    }
                    print_success(&format!(
                        "已新增并切换到 {} ({})",
                        active_model.name, active_model.model
                    ));
                }
                Err(err) => print_error(&err.to_string()),
            }
            continue;
        }
        if let Some(selector) = input.strip_prefix("/model ") {
            match resolve_model(&models, Some(selector.trim())) {
                Ok(model) => {
                    active_model = model;
                    if let Some(id) = conversation_id.as_deref() {
                        bind_conversation_model(&db, id, Some(&active_model.id))?;
                    }
                    print_success(&format!(
                        "已切换到 {} ({})",
                        active_model.name, active_model.model
                    ));
                }
                Err(err) => print_error(&err.to_string()),
            }
            continue;
        }

        if let Err(err) = ask(
            &db,
            &active_model,
            project.as_deref(),
            project_session_path.as_deref(),
            &mut conversation_id,
            &mut history,
            &input,
        )
        .await
        {
            print_error(&err.to_string());
        }
    }
    Ok(())
}

#[cfg(not(test))]
fn start_profile_worker(db_path: PathBuf) {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                crate::logging::warn(
                    "profile-cli-worker",
                    "runtime creation failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
                return;
            }
        };
        let db = match Database::open(db_path) {
            Ok(db) => db,
            Err(error) => {
                crate::logging::warn(
                    "profile-cli-worker",
                    "database open failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
                return;
            }
        };
        runtime.block_on(async move {
            loop {
                if let Err(error) = crate::profile::run_database_worker_cycle(&db).await {
                    crate::logging::warn(
                        "profile-cli-worker",
                        "background cycle failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
        });
    });
}

#[cfg(test)]
fn start_profile_worker(_db_path: PathBuf) {}

fn resolve_requested_conversation(
    db: &Database,
    project_path: Option<&str>,
    continue_latest: bool,
    resume: Option<&str>,
) -> AppResult<Option<Conversation>> {
    if !continue_latest && resume.is_none() {
        return Ok(None);
    }
    let conversations = db.list_conversations(project_path)?;
    if continue_latest {
        return conversations.into_iter().next().map(Some).ok_or_else(|| {
            AppError::Message("当前项目没有可恢复的会话；使用 nano 开始新会话".to_string())
        });
    }

    let selector = resume.unwrap_or_default().trim();
    let exact = conversations
        .iter()
        .find(|conversation| conversation.id == selector)
        .cloned();
    if exact.is_some() {
        return Ok(exact);
    }
    let prefix_matches = conversations
        .into_iter()
        .filter(|conversation| conversation.id.starts_with(selector))
        .collect::<Vec<_>>();
    match prefix_matches.as_slice() {
        [conversation] => Ok(Some(conversation.clone())),
        [] => Err(AppError::Message(format!(
            "当前项目未找到会话 {selector}；使用 nano --sessions 查看会话"
        ))),
        _ => Err(AppError::Message(format!(
            "会话 ID 前缀 {selector} 不唯一，请提供更多字符"
        ))),
    }
}

fn load_conversation_history(db: &Database, conversation_id: &str) -> AppResult<Vec<Message>> {
    service_load_conversation_history(db, conversation_id)
}

async fn rebuild_project_indexes(db: &Database, root: &Path) -> AppResult<()> {
    let canonical = root.to_string_lossy().to_string();
    print_status(&format!("正在索引项目 {}", display_project_path(root)));
    let embedding_config = db.get_model_config(EMBEDDING_CONFIG_ID).ok();
    let code = prepare_project_code_index(root, &canonical, embedding_config.as_ref()).await?;
    persist_project_code_index(db, &canonical, &code)?;
    let documents =
        prepare_project_document_index(root, &canonical, embedding_config.as_ref()).await?;
    persist_project_document_index(db, &canonical, &documents)?;
    print_status("项目索引已就绪");
    Ok(())
}

async fn ask(
    db: &Database,
    model: &ModelConfig,
    project: Option<&Path>,
    project_session_path: Option<&str>,
    conversation_id: &mut Option<String>,
    history: &mut Vec<Message>,
    input: &str,
) -> AppResult<()> {
    let system = build_system_message(db, project, input).await?;
    let user_message = ChatMessage {
        role: "user".to_string(),
        content: input.to_string(),
    };
    let user_history_message = if let Some(project_session_path) = project_session_path {
        if conversation_id.is_none() {
            let conversation = create_conversation(
                db,
                ConversationDraft {
                    title: Some("New chat".to_string()),
                    model_config_id: Some(model.id.clone()),
                    project_path: Some(project_session_path.to_string()),
                },
            )?;
            *conversation_id = Some(conversation.id);
        }
        let persistent_id = conversation_id
            .as_deref()
            .ok_or_else(|| AppError::Message("创建项目会话后未获得会话 ID".to_string()))?;
        append_conversation_message(
            db,
            MessageDraft {
                conversation_id: persistent_id.to_string(),
                role: user_message.role.clone(),
                content: user_message.content.clone(),
                metadata: None,
            },
        )?
    } else {
        transient_message(&user_message.role, &user_message.content)
    };
    let mut planning_history = history.clone();
    planning_history.push(user_history_message.clone());
    let prepared = prepare_cli_context(
        db,
        model,
        conversation_id.as_deref(),
        system,
        &planning_history,
        input,
    )
    .await?;
    let mut messages = Vec::with_capacity(prepared.context_messages.len() + 1);
    messages.push(prepared.system_message);
    messages.extend(prepared.context_messages.iter().map(message_to_chat));

    let request_id = Uuid::new_v4().to_string();
    let lease_owner = format!("cli-chat-{request_id}");
    let request = ChatStreamRequest {
        request_id,
        model_config_id: model.id.clone(),
        messages,
        temperature: None,
        trace_id: None,
        max_tokens: Some(prepared.output_reserve),
        top_p: None,
        reasoning_effort: None,
    };
    let mut answer = String::new();
    let mut stream_error = None;
    print!("\n{} ", CliTheme::stdout().brand("nano:"));
    io::stdout().flush()?;
    db.start_profile_foreground_lease(&lease_owner, 30)?;
    let stream_result = {
        let stream = stream_chat_completion(model.clone(), request, |event| {
            match event {
                ChatStreamEvent::Delta { content, .. } => {
                    print!("{content}");
                    io::stdout().flush()?;
                    answer.push_str(&content);
                }
                ChatStreamEvent::Error { message, .. } => stream_error = Some(message),
                ChatStreamEvent::ReasoningDelta { .. }
                | ChatStreamEvent::Done { .. }
                | ChatStreamEvent::Interrupted { .. } => {}
            }
            Ok(())
        });
        tokio::pin!(stream);
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), &mut stream).await {
                Ok(result) => break result,
                Err(_) => db.start_profile_foreground_lease(&lease_owner, 30)?,
            }
        }
    };
    db.finish_profile_foreground_lease(&lease_owner)?;
    stream_result?;
    println!("\n");
    if let Some(message) = stream_error {
        return Err(AppError::Message(message));
    }
    if answer.trim().is_empty() {
        return Err(AppError::Message("模型返回了空响应".to_string()));
    }
    let assistant_message = ChatMessage {
        role: "assistant".to_string(),
        content: answer,
    };
    let assistant_history_message = if project_session_path.is_some() {
        let persistent_id = conversation_id
            .as_deref()
            .ok_or_else(|| AppError::Message("保存项目会话时缺少会话 ID".to_string()))?;
        append_conversation_message(
            db,
            MessageDraft {
                conversation_id: persistent_id.to_string(),
                role: assistant_message.role.clone(),
                content: assistant_message.content.clone(),
                metadata: None,
            },
        )?
    } else {
        transient_message(&assistant_message.role, &assistant_message.content)
    };
    history.push(user_history_message);
    if let Some(summary) = prepared.created_summary {
        history.push(summary);
    }
    history.push(assistant_history_message);
    Ok(())
}

fn transient_message(role: &str, content: &str) -> Message {
    Message {
        id: Uuid::new_v4().to_string(),
        conversation_id: String::new(),
        role: role.to_string(),
        content: content.to_string(),
        metadata: None,
        created_at: chrono::Utc::now(),
    }
}

fn message_to_chat(message: &Message) -> ChatMessage {
    ChatMessage {
        role: message.role.clone(),
        content: message.content.clone(),
    }
}

async fn build_system_message(
    db: &Database,
    project: Option<&Path>,
    query: &str,
) -> AppResult<ChatMessage> {
    let base_context = load_base_context_for_db(db, project, query).await?;
    let memory_context = base_context
        .memories
        .iter()
        .map(|memory| format!("- {}: {}", memory.title, memory.content))
        .collect::<Vec<_>>()
        .join("\n");
    let session_instruction = if project.is_some() {
        format!(
            "你是 {} 的终端助手。回答应准确、简明、可执行。当前项目会话会保存到 {} 本地数据库，可在退出后恢复。",
            brand::DISPLAY_NAME,
            brand::DISPLAY_NAME
        )
    } else {
        format!(
            "你是 {} 的终端助手。回答应准确、简明、可执行。当前临时会话只保存在进程内，退出后不会写入对话历史。",
            brand::DISPLAY_NAME
        )
    };
    let mut sections = vec![session_instruction];
    if let Some(profile_context) = base_context.profile_context {
        sections.push(profile_context);
    }
    if !memory_context.is_empty() {
        sections.push(format!(
            "用户相关记忆（仅在与问题有关时使用）：\n{memory_context}"
        ));
    }

    if let Some(root) = project {
        let file_context = base_context
            .project_files
            .iter()
            .take(160)
            .map(|entry| {
                if entry.is_dir {
                    format!("- {}/", entry.path)
                } else {
                    format!("- {}", entry.path)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let code_context = base_context
            .code_matches
            .iter()
            .map(|item| {
                format!(
                    "[{}:{}-{}] {} {}\n{}",
                    item.file_path,
                    item.start_line,
                    item.end_line,
                    item.kind,
                    item.name,
                    item.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let document_context = base_context
            .project_index_matches
            .iter()
            .map(|item| {
                format!(
                    "[{}:{}-{}] {}\n{}",
                    item.file_path, item.start_line, item.end_line, item.title, item.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!(
            "当前项目根目录：{}\n回答项目问题时必须以此目录和下列检索证据为准，不要编造未提供的实现。引用代码时给出项目相对路径和行号。\n\n项目文件（最多 160 项）：\n{}",
            display_project_path(root),
            if file_context.is_empty() { "（空）" } else { &file_context }
        ));
        if !code_context.is_empty() {
            sections.push(format!("当前问题的代码索引结果：\n{code_context}"));
        }
        if !document_context.is_empty() {
            sections.push(format!("当前问题的文档索引结果：\n{document_context}"));
        }
    } else {
        sections.push(
            "当前是普通临时对话，没有绑定项目目录；不要声称已经读取或修改本地项目。".to_string(),
        );
    }

    Ok(ChatMessage {
        role: "system".to_string(),
        content: sections.join("\n\n"),
    })
}

fn default_app_data_dir() -> AppResult<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join(brand::IDENTIFIER))
            .ok_or_else(|| AppError::Message("无法确定 APPDATA 目录".to_string()))
    }
    #[cfg(target_os = "macos")]
    {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| {
                path.join("Library")
                    .join("Application Support")
                    .join(brand::IDENTIFIER)
            })
            .ok_or_else(|| AppError::Message("无法确定 HOME 目录".to_string()));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(path).join(brand::IDENTIFIER));
        }
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".local").join("share").join(brand::IDENTIFIER))
            .ok_or_else(|| AppError::Message("无法确定 HOME 目录".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_temporary_one_shot_session() {
        let options = parse_args(args(&["--temp", "-p", "hello", "--model", "local"]))
            .expect("options should parse");
        assert_eq!(options.mode, SessionMode::Temporary);
        assert_eq!(options.prompt.as_deref(), Some("hello"));
        assert_eq!(options.model.as_deref(), Some("local"));
    }

    #[test]
    fn parses_explicit_project_session() {
        let options = parse_args(args(&["-C", "D:/workspace/demo", "question"]))
            .expect("options should parse");
        assert_eq!(
            options.mode,
            SessionMode::Project(PathBuf::from("D:/workspace/demo"))
        );
        assert_eq!(options.prompt.as_deref(), Some("question"));
    }

    #[test]
    fn rejects_conflicting_session_modes() {
        let error = parse_args(args(&["--temp", "--project", "."]))
            .expect_err("conflicting modes should fail");
        assert!(error.contains("不能同时使用"));
    }

    #[test]
    fn parses_session_recovery_options() {
        let continue_options = parse_args(args(&["--continue"])).expect("continue should parse");
        assert!(continue_options.continue_latest);

        let resume_options =
            parse_args(args(&["--resume", "1234abcd"])).expect("resume should parse");
        assert_eq!(resume_options.resume.as_deref(), Some("1234abcd"));

        let list_options = parse_args(args(&["--sessions"])).expect("sessions should parse");
        assert!(list_options.list_sessions);

        let show_options = parse_args(args(&["--show", "1234abcd"])).expect("show should parse");
        assert_eq!(show_options.show_session.as_deref(), Some("1234abcd"));

        let file_options = parse_args(args(&["--files"])).expect("files should parse");
        assert!(file_options.list_files);
    }

    #[test]
    fn temporary_mode_rejects_session_recovery() {
        let error = parse_args(args(&["--temp", "--continue"]))
            .expect_err("temporary recovery should fail");
        assert!(error.contains("--temp 不支持"));

        let error = parse_args(args(&["--temp", "--files"]))
            .expect_err("temporary file listing should fail");
        assert!(error.contains("--temp 不支持"));
    }

    #[test]
    fn rejects_conflicting_inspection_options() {
        let error = parse_args(args(&["--sessions", "--files"]))
            .expect_err("inspection options should be exclusive");
        assert!(error.contains("不能同时使用"));

        let error = parse_args(args(&["--show", "1234abcd", "question"]))
            .expect_err("show and prompt should be exclusive");
        assert!(error.contains("不能与恢复参数或问题同时使用"));
    }

    #[test]
    fn inspection_commands_do_not_require_a_model() {
        let root = env::temp_dir().join(format!("nano-cli-inspect-{}", Uuid::new_v4()));
        let project = root.join("project");
        let data_dir = root.join("data");
        std::fs::create_dir_all(&project).expect("temporary project should be created");
        std::fs::create_dir_all(&data_dir).expect("temporary data directory should be created");
        std::fs::write(project.join("README.md"), "# demo")
            .expect("temporary project file should be written");

        let canonical_project =
            project_root(&project.to_string_lossy()).expect("project should resolve");
        let project_path = display_project_path(&canonical_project);
        let db =
            Database::open(data_dir.join(brand::MAIN_DATABASE_NAME)).expect("database should open");
        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("Inspect me".to_string()),
                model_config_id: None,
                project_path: Some(project_path),
            })
            .expect("conversation should be created");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "user".to_string(),
            content: "hello".to_string(),
            metadata: None,
        })
        .expect("message should persist");
        drop(db);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        runtime
            .block_on(run_session(CliOptions {
                mode: SessionMode::Project(project.clone()),
                data_dir: Some(data_dir.clone()),
                show_session: Some(short_session_id(&conversation.id).to_string()),
                ..CliOptions::default()
            }))
            .expect("show should work without a model");
        runtime
            .block_on(run_session(CliOptions {
                mode: SessionMode::Project(project),
                data_dir: Some(data_dir),
                list_files: true,
                ..CliOptions::default()
            }))
            .expect("file listing should work without a model");

        std::fs::remove_dir_all(root).expect("temporary files should be removed");
    }

    #[test]
    fn first_use_model_setup_runs_only_when_chat_models_are_missing() {
        let root = env::temp_dir().join(format!("nano-cli-model-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");
        let mut setup_called = false;

        let models = ensure_chat_models(&db, |db| {
            setup_called = true;
            db.save_model_config(ModelConfigDraft {
                id: Some("first-model".to_string()),
                name: "First model".to_string(),
                provider: "openai-compatible".to_string(),
                base_url: "http://localhost:11434/v1".to_string(),
                model: "local-model".to_string(),
                api_key: String::new(),
                temperature: 0.4,
                max_tokens: None,
                context_window: 32_768,
                top_p: None,
                reasoning_effort: String::new(),
                model_kind: "chat".to_string(),
                routing_group: "默认组".to_string(),
                routing_enabled: true,
                routing_cost: 3,
                routing_quality: 3,
                routing_speed: 3,
                routing_tasks: Vec::new(),
                embedding_provider: String::new(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                embedding_api_key: String::new(),
            })
        })
        .expect("first-use setup should create a model");

        assert!(setup_called);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "first-model");

        let existing = ensure_chat_models(&db, |_| {
            panic!("setup must not run when a chat model already exists")
        })
        .expect("existing model should be reused");
        assert_eq!(existing.len(), 1);

        drop(db);
        std::fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn adds_model_refreshes_list_and_supports_switch_selectors() {
        let root = env::temp_dir().join(format!("nano-cli-add-model-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");
        let save_model = |id: &str, name: &str, model: &str| {
            db.save_model_config(ModelConfigDraft {
                id: Some(id.to_string()),
                name: name.to_string(),
                provider: "openai-compatible".to_string(),
                base_url: "http://localhost:11434/v1".to_string(),
                model: model.to_string(),
                api_key: String::new(),
                temperature: 0.4,
                max_tokens: None,
                context_window: 32_768,
                top_p: None,
                reasoning_effort: String::new(),
                model_kind: "chat".to_string(),
                routing_group: "默认组".to_string(),
                routing_enabled: true,
                routing_cost: 3,
                routing_quality: 3,
                routing_speed: 3,
                routing_tasks: Vec::new(),
                embedding_provider: String::new(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                embedding_api_key: String::new(),
            })
        };
        save_model("first-model", "First", "model-a").expect("first model should save");
        let mut models = chat_models(&db).expect("models should load");

        let added = add_chat_model(&db, &mut models, |_| {
            save_model("second-model", "Second", "model-b")
        })
        .expect("second model should be added and selected");

        assert_eq!(models.len(), 2);
        assert_eq!(added.id, "second-model");
        assert_eq!(
            resolve_model(&models, Some("Second"))
                .expect("configuration name should select")
                .id,
            "second-model"
        );
        assert_eq!(
            resolve_model(&models, Some("model-a"))
                .expect("model name should select")
                .id,
            "first-model"
        );
        assert_eq!(
            resolve_model(&models, Some("first-model"))
                .expect("id should select")
                .id,
            "first-model"
        );

        drop(db);
        std::fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn cli_theme_adds_colors_only_when_enabled() {
        let colored = CliTheme { enabled: true }.brand("Nano");
        let plain = CliTheme { enabled: false }.brand("Nano");

        assert_eq!(colored, "\x1b[1;36mNano\x1b[0m");
        assert_eq!(plain, "Nano");
    }

    #[test]
    fn restores_persisted_project_conversation_history() {
        let root = env::temp_dir().join(format!("nano-cli-session-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");
        let project_path = "D:\\workspace\\demo";
        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("New chat".to_string()),
                model_config_id: None,
                project_path: Some(project_path.to_string()),
            })
            .expect("conversation should be created");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "user".to_string(),
            content: "first question".to_string(),
            metadata: None,
        })
        .expect("user message should persist");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "assistant".to_string(),
            content: "first answer".to_string(),
            metadata: None,
        })
        .expect("assistant message should persist");

        let resumed = resolve_requested_conversation(&db, Some(project_path), true, None)
            .expect("latest conversation should resolve")
            .expect("conversation should exist");
        let history =
            load_conversation_history(&db, &resumed.id).expect("conversation history should load");
        let prefix = short_session_id(&conversation.id);
        let resumed_by_prefix =
            resolve_requested_conversation(&db, Some(project_path), false, Some(prefix))
                .expect("conversation prefix should resolve")
                .expect("conversation should exist");

        assert_eq!(resumed.id, conversation.id);
        assert_eq!(resumed_by_prefix.id, conversation.id);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "first question");
        assert_eq!(history[1].content, "first answer");
        drop(db);
        std::fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn project_ask_creates_and_persists_a_restorable_conversation() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let root = env::temp_dir().join(format!("nano-cli-ask-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary project should be created");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
        let address = listener.local_addr().expect("mock address should resolve");
        let (request_sender, request_receiver) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("mock request should arrive");
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let read = stream.read(&mut chunk).expect("request should be readable");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                let Some(header_end) = request.windows(4).position(|item| item == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                });
                if content_length.is_none_or(|length| request.len() >= header_end + 4 + length) {
                    break;
                }
            }
            request_sender
                .send(String::from_utf8_lossy(&request).to_string())
                .expect("request should be captured");
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"saved answer\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("mock response should be written");
        });
        let now = chrono::Utc::now();
        let model = ModelConfig {
            id: "mock-model".to_string(),
            name: "Mock".to_string(),
            provider: "openai-compatible".to_string(),
            base_url: format!("http://{address}/v1"),
            model: "mock".to_string(),
            api_key: "test".to_string(),
            temperature: 0.4,
            max_tokens: None,
            context_window: 32_768,
            top_p: None,
            reasoning_effort: String::new(),
            model_kind: "chat".to_string(),
            routing_group: "默认组".to_string(),
            routing_enabled: true,
            routing_cost: 3,
            routing_quality: 3,
            routing_speed: 3,
            routing_tasks: Vec::new(),
            embedding_provider: String::new(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            embedding_api_key: String::new(),
            created_at: now,
            updated_at: now,
        };
        db.save_model_config(crate::models::ModelConfigDraft {
            id: Some(model.id.clone()),
            name: model.name.clone(),
            provider: model.provider.clone(),
            base_url: model.base_url.clone(),
            model: model.model.clone(),
            api_key: model.api_key.clone(),
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            context_window: model.context_window,
            top_p: model.top_p,
            reasoning_effort: model.reasoning_effort.clone(),
            model_kind: model.model_kind.clone(),
            routing_group: model.routing_group.clone(),
            routing_enabled: model.routing_enabled,
            routing_cost: model.routing_cost,
            routing_quality: model.routing_quality,
            routing_speed: model.routing_speed,
            routing_tasks: model.routing_tasks.clone(),
            embedding_provider: String::new(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            embedding_api_key: String::new(),
        })
        .expect("model config should persist for the conversation foreign key");
        let project_path = display_project_path(&root);
        let mut conversation_id = None;
        let mut history = Vec::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        runtime
            .block_on(ask(
                &db,
                &model,
                Some(&root),
                Some(&project_path),
                &mut conversation_id,
                &mut history,
                "first question",
            ))
            .expect("project question should complete");
        server.join().expect("mock server should finish");
        let captured_request = request_receiver
            .recv()
            .expect("captured request should be available");

        let conversation_id = conversation_id.expect("conversation should be created");
        let messages = db
            .list_messages(&conversation_id)
            .expect("messages should load");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "first question");
        assert_eq!(messages[1].content, "saved answer");
        assert_eq!(history.len(), 2);
        assert_eq!(captured_request.matches("first question").count(), 1);
        drop(db);
        std::fs::remove_dir_all(root).expect("temporary project should be removed");
    }

    #[test]
    fn rebuilds_code_and_document_indexes_for_project_questions() {
        let root = env::temp_dir().join(format!("nano-cli-project-{}", Uuid::new_v4()));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).expect("project directory should be created");
        std::fs::write(
            source_dir.join("main.rs"),
            "fn greet_user() -> &'static str { \"hello\" }\n",
        )
        .expect("source should be written");
        std::fs::write(root.join("README.md"), "# Demo\nRun with cargo run.\n")
            .expect("readme should be written");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        runtime
            .block_on(rebuild_project_indexes(&db, &root))
            .expect("indexes should rebuild");
        let canonical = root.to_string_lossy().to_string();
        let code = db
            .search_code_index(&canonical, "greet_user", None, 8)
            .expect("code search should work");
        let documents = db
            .search_project_index(&canonical, Some(DOCUMENT_INDEXER), "cargo run", None, 6)
            .expect("document search should work");

        assert!(code.iter().any(|item| item.file_path == "src/main.rs"));
        assert!(documents.iter().any(|item| item.file_path == "README.md"));
        drop(db);
        std::fs::remove_dir_all(root).expect("temporary project should be removed");
    }

    #[test]
    fn temporary_system_message_does_not_bind_a_project() {
        let root = env::temp_dir().join(format!("nano-cli-temp-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let db = Database::open(root.join("nano-test.sqlite3")).expect("database should open");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let message = runtime
            .block_on(build_system_message(&db, None, "hello"))
            .expect("system message should build");

        assert!(message.content.contains("普通临时对话"));
        assert!(!message.content.contains("当前项目根目录"));
        drop(db);
        std::fs::remove_dir_all(root).expect("temporary directory should be removed");
    }
}
