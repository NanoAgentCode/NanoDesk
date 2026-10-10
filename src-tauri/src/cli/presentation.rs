use super::*;

pub(super) fn print_banner(
    model: &ModelConfig,
    project: Option<&Path>,
    resumed_conversation: Option<&Conversation>,
) {
    let theme = CliTheme::stdout();
    println!(
        "{} {}",
        theme.brand("◆ Nano CLI"),
        theme.accent(format!("· {} ({})", model.name, model.model))
    );
    match project {
        Some(path) => {
            println!("{} {}", theme.label("项目："), display_project_path(path));
            match resumed_conversation {
                Some(conversation) => println!(
                    "{} {} {} · {}",
                    theme.label("会话："),
                    theme.success("已恢复"),
                    theme.command(short_session_id(&conversation.id)),
                    conversation.title,
                ),
                None => println!("{} 新会话（首次发送消息时保存）", theme.label("会话：")),
            }
        }
        None => println!("{} 普通临时对话（不保存会话）", theme.label("模式：")),
    }
    println!(
        "{}\n",
        theme.muted(format!(
            "输入 {} 查看命令，{} 退出。",
            theme.command("/help"),
            theme.command("/exit")
        ))
    );
}

pub(super) fn print_sessions(sessions: &[Conversation]) {
    let theme = CliTheme::stdout();
    if sessions.is_empty() {
        println!("{} 当前项目没有可恢复的会话。", theme.command("!"));
        return;
    }
    println!("{}", theme.brand("当前项目可恢复会话"));
    for session in sessions {
        println!(
            "{}  {}  {}",
            theme.command(short_session_id(&session.id)),
            theme.muted(session.updated_at.format("%Y-%m-%d %H:%M").to_string()),
            session.title
        );
    }
    println!(
        "\n{}",
        theme.muted(format!(
            "使用 {} 恢复，或 {} 恢复最近会话。",
            theme.command("nano --resume <会话ID>"),
            theme.command("nano --continue")
        ))
    );
}

pub(super) fn print_conversation(conversation: &Conversation, messages: &[Message]) {
    let theme = CliTheme::stdout();
    println!("{}", theme.brand("会话详情"));
    println!("{} {}", theme.label("ID："), conversation.id);
    println!("{} {}", theme.label("标题："), conversation.title);
    println!(
        "{} {}",
        theme.label("更新时间："),
        conversation.updated_at.format("%Y-%m-%d %H:%M:%S")
    );
    if let Some(model_id) = conversation.model_config_id.as_deref() {
        println!("{} {}", theme.label("模型配置："), model_id);
    }
    println!();
    if messages.is_empty() {
        println!("{} 当前会话还没有消息。", theme.command("!"));
        return;
    }
    for message in messages {
        let role = match message.role.as_str() {
            "user" => theme.prompt("你"),
            "assistant" => theme.brand("nano"),
            other => theme.accent(other),
        };
        println!(
            "{} {}",
            role,
            theme.muted(message.created_at.format("%Y-%m-%d %H:%M:%S").to_string())
        );
        println!("{}\n", message.content);
    }
}

pub(super) fn print_project_files(root: &Path, files: &[ProjectFileEntry]) {
    let theme = CliTheme::stdout();
    println!(
        "{} {}",
        theme.brand("项目文件"),
        theme.muted(display_project_path(root))
    );
    if files.is_empty() {
        println!("{} 当前项目没有可显示的文件。", theme.command("!"));
        return;
    }
    for file in files {
        if file.is_dir {
            println!("{}/", file.path);
        } else if let Some(size) = file.size {
            println!("{}  {}", file.path, theme.muted(format!("{size} B")));
        } else {
            println!("{}", file.path);
        }
    }
    println!(
        "\n{} 共 {} 项（最多显示 300 项）。",
        theme.muted("·"),
        files.len()
    );
}

pub(super) fn short_session_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

pub(super) fn display_project_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        path.strip_prefix(r"\\?\")
            .unwrap_or(path.as_ref())
            .to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_string()
    }
}

pub(super) fn print_models(models: &[ModelConfig], active: &ModelConfig) {
    let theme = CliTheme::stdout();
    for model in models {
        let marker = if model.id == active.id {
            theme.success("●")
        } else {
            theme.muted("○")
        };
        println!(
            "{marker} {} · {} · id={}",
            model.name,
            theme.accent(&model.model),
            theme.muted(&model.id)
        );
    }
}

pub(super) fn print_interactive_help() {
    let theme = CliTheme::stdout();
    println!("{}", theme.brand("交互命令"));
    for (command, description) in [
        ("/help", "显示交互命令"),
        ("/clear", "结束当前项目会话或清空临时上下文"),
        ("/model", "查看可用模型"),
        ("/model add", "新增模型并立即切换"),
        ("/model <名称>", "按配置名称、模型名或 ID 切换模型"),
        ("/exit", "退出 nano"),
    ] {
        println!(
            "  {} {}",
            theme.command(format!("{command:<18}")),
            description
        );
    }
}

pub(super) fn print_help() {
    let theme = CliTheme::stdout();
    println!(
        "{}",
        theme.brand(format!("◆ {} 终端交互客户端", brand::DISPLAY_NAME))
    );
    println!();
    println!("{}", theme.label("用法"));
    println!("  {}", theme.command("nano [选项] [问题]"));
    println!();
    println!("{}", theme.label("默认行为"));
    println!(
        "  在当前目录启动项目问答，并将项目会话保存到 {} 本地数据库。",
        brand::DISPLAY_NAME
    );
    println!("  首次使用且没有聊天模型时，将引导完成模型配置。");
    println!();
    println!("{}", theme.label("选项"));
    for (option, description) in [
        ("-C, --project <目录>", "指定项目目录"),
        ("    --temp", "启动不绑定项目、不保存历史的临时对话"),
        ("    --continue", "恢复当前项目最近会话"),
        (
            "    --resume <会话ID>",
            "恢复当前项目指定会话（支持唯一前缀）",
        ),
        ("    --sessions", "列出当前项目可恢复会话"),
        ("    --show <会话ID>", "查看指定会话详情和历史消息"),
        ("    --files", "列出当前项目文件"),
        ("-p, --prompt <问题>", "单次提问后退出"),
        ("-m, --model <模型>", "按名称、模型名或 ID 选择模型"),
        ("    --no-index", "使用已有项目索引，不在启动时重建"),
        ("    --data-dir <目录>", "覆盖应用数据目录"),
        ("-h, --help", "显示帮助"),
        ("-V, --version", "显示版本"),
    ] {
        println!(
            "  {} {}",
            theme.command(format!("{option:<25}")),
            description
        );
    }
    println!();
    println!("{}", theme.label("示例"));
    for example in [
        "nano",
        "nano --continue",
        "nano --sessions",
        "nano --show 1234abcd",
        "nano --files",
        "nano --resume 1234abcd",
        r"nano --project D:\workspace\demo --continue",
        "nano --temp",
        "nano -p \"这个项目的启动入口在哪里？\"",
        "nano --temp -p \"帮我写一个周报提纲\"",
    ] {
        println!("  {}", theme.command(example));
    }
}

pub(super) fn print_error(message: &str) {
    let theme = CliTheme::stderr();
    eprintln!("{} {message}", theme.error("✗ nano:"));
}

pub(super) fn print_warning(message: &str) {
    let theme = CliTheme::stderr();
    eprintln!("{} {message}", theme.command("! nano:"));
}

pub(super) fn print_status(message: &str) {
    let theme = CliTheme::stderr();
    eprintln!("{} {message}", theme.accent("• nano:"));
}

pub(super) fn print_success(message: &str) {
    let theme = CliTheme::stdout();
    println!("{} {message}", theme.success("✓"));
}
