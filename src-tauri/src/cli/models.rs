use super::*;
use std::io::IsTerminal;

pub(super) fn chat_models(db: &Database) -> AppResult<Vec<ModelConfig>> {
    Ok(db
        .list_model_configs()?
        .into_iter()
        .filter(|model| {
            model.id != EMBEDDING_CONFIG_ID
                && matches!(model.model_kind.as_str(), "chat" | "both")
                && !crate::llm::is_asr_model_id(&model.model)
        })
        .collect())
}

pub(super) fn ensure_chat_models<F>(db: &Database, configure: F) -> AppResult<Vec<ModelConfig>>
where
    F: FnOnce(&Database) -> AppResult<ModelConfig>,
{
    let models = chat_models(db)?;
    if !models.is_empty() {
        return Ok(models);
    }
    configure(db)?;
    chat_models(db)
}

pub(super) fn configure_initial_model(db: &Database) -> AppResult<ModelConfig> {
    configure_model(
        db,
        format!("◆ {} 首次配置", brand::DISPLAY_NAME),
        "尚未发现聊天模型。完成下面几项配置后即可开始使用。",
    )
}

pub(super) fn configure_additional_model(db: &Database) -> AppResult<ModelConfig> {
    configure_model(db, "◆ 新增聊天模型".to_string(), "填写新的模型配置。")
}

pub(super) fn configure_model(
    db: &Database,
    title: String,
    description: &str,
) -> AppResult<ModelConfig> {
    if !io::stdin().is_terminal() {
        return Err(AppError::Message(format!(
            "模型配置需要交互式终端。请直接运行 nano，或在 {} 桌面端的“设置 > 模型”中添加模型",
            brand::DISPLAY_NAME
        )));
    }

    let theme = CliTheme::stdout();
    println!("{}", theme.brand(title));
    println!("{}", theme.muted(description));
    println!();

    let provider_choice = loop {
        println!("  {}", theme.label("模型协议"));
        println!("    {}  OpenAI 兼容协议", theme.command("1"));
        println!("    {}  Anthropic 兼容协议", theme.command("2"));
        let value = prompt_line(&theme, "请选择", Some("1"))?;
        match value.as_str() {
            "1" => break "openai-compatible",
            "2" => break "anthropic",
            _ => println!("  {} 请输入 1 或 2。", theme.command("!")),
        }
    };
    let (default_name, default_url, default_model) = match provider_choice {
        "anthropic" => (
            "Anthropic",
            "https://api.anthropic.com",
            "claude-3-5-sonnet-latest",
        ),
        _ => ("OpenAI", "https://api.openai.com/v1", "gpt-4o-mini"),
    };
    let name = prompt_line(&theme, "配置名称", Some(default_name))?;
    let base_url = prompt_line(&theme, "接口地址", Some(default_url))?;
    let model = prompt_line(&theme, "模型名称", Some(default_model))?;
    let context_window = loop {
        let value = prompt_line(
            &theme,
            "上下文窗口 Token（以服务商文档为准）",
            Some("32768"),
        )?;
        match value.parse::<u32>() {
            Ok(value) if value >= 2_048 => break value,
            _ => println!("  {} 请输入不小于 2048 的整数。", theme.command("!")),
        }
    };
    let api_key = loop {
        let prompt = format!("  {} ", theme.prompt("API Key（隐藏输入）:"));
        let value = rpassword::prompt_password(prompt)
            .map_err(|err| AppError::Message(format!("读取 API Key 失败: {err}")))?;
        let value = value.trim().to_string();
        if !value.is_empty() || base_url.contains("localhost") {
            break value;
        }
        println!("  {} 远程模型需要填写 API Key。", theme.command("!"));
    };

    let config = db.save_model_config(ModelConfigDraft {
        id: None,
        name,
        provider: provider_choice.to_string(),
        base_url,
        model,
        api_key,
        temperature: 0.4,
        max_tokens: None,
        context_window,
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
    })?;
    println!();
    println!(
        "{} {} ({})",
        theme.success("✓ 模型配置已保存："),
        config.name,
        config.model
    );
    println!();
    Ok(config)
}

pub(super) fn prompt_line(
    theme: &CliTheme,
    label: &str,
    default: Option<&str>,
) -> AppResult<String> {
    match default {
        Some(default) => print!(
            "  {} {} ",
            theme.prompt(format!("{label}:")),
            theme.muted(format!("[{default}]"))
        ),
        None => print!("  {} ", theme.prompt(format!("{label}:"))),
    }
    io::stdout().flush()?;
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value)?;
    if read == 0 {
        return Err(AppError::Message("模型配置已取消".to_string()));
    }
    let value = value.trim();
    if value.is_empty() {
        default
            .map(str::to_string)
            .ok_or_else(|| AppError::Message(format!("{label}不能为空")))
    } else {
        Ok(value.to_string())
    }
}

pub(super) fn add_chat_model<F>(
    db: &Database,
    models: &mut Vec<ModelConfig>,
    configure: F,
) -> AppResult<ModelConfig>
where
    F: FnOnce(&Database) -> AppResult<ModelConfig>,
{
    let added = configure(db)?;
    *models = chat_models(db)?;
    resolve_model(models, Some(&added.id))
}

pub(super) fn resolve_model(
    models: &[ModelConfig],
    selector: Option<&str>,
) -> AppResult<ModelConfig> {
    let selected = match selector.map(str::trim).filter(|value| !value.is_empty()) {
        None => models.first(),
        Some(selector) => models.iter().find(|model| {
            model.id == selector
                || model.name.eq_ignore_ascii_case(selector)
                || model.model.eq_ignore_ascii_case(selector)
        }),
    };
    selected.cloned().ok_or_else(|| {
        AppError::Message(format!(
            "未找到模型{}；使用 /model 查看可用模型",
            selector
                .map(|value| format!("“{value}”"))
                .unwrap_or_default()
        ))
    })
}

pub(super) fn resolve_session_model(
    models: &[ModelConfig],
    requested: Option<&str>,
    saved_model_id: Option<&str>,
) -> AppResult<ModelConfig> {
    if requested.is_some() {
        return resolve_model(models, requested);
    }
    if let Some(saved_model_id) = saved_model_id {
        if let Some(model) = models.iter().find(|model| model.id == saved_model_id) {
            return Ok(model.clone());
        }
        print_warning("已保存的模型配置不存在，已切换到默认模型");
    }
    resolve_model(models, None)
}
