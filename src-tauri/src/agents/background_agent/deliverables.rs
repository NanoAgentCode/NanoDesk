//! Goal-aware file roles emitted by the model; disk validation remains authoritative.
use crate::models::DeliverableFile;
const OPEN: &str = "<deliverables>";
pub(super) const INSTRUCTION: &str = r#"
文件交付协议：根据当前用户目标和本轮文件用途判断文件角色，不能根据扩展名或有无下载链接判断。
例如先要求写诗到 MD：poem.md 是 final；后续要求转 DOCX：转换脚本是 intermediate，poem.docx 是 final，原 MD 的历史交付身份保持。
如果用户一开始就要求写成 Word 文档，即使过程先生成了 MD，临时 MD 和转换脚本都是 intermediate，DOCX 才是 final；不要因为某文件最先产生就标为交付。
如果用户要求交付脚本本身，脚本就是 final。将之前交付物用作输入，不要把它降级。仅标记当前工作目录中已成功生成、修改或确认存在的文件，路径必须相对当前工作目录。
完成文件任务时，在回答末尾单独一行输出内部协议（不要放入代码块）：
<deliverables>[{"path":"poem.docx","kind":"final"},{"path":"convert.js","kind":"intermediate"}]</deliverables>
没有文件则输出 <deliverables>[]</deliverables>。协议不会展示给用户；正文正常说明结果，文件链接可选。未执行或失败的操作不能声称交付成功。
"#;

pub(super) fn visible_content(content: &str) -> String {
    let visible = crate::task_summary::visible_content(content);
    if let Some(start) = crate::task_summary::protocol_start(&visible, true, OPEN) {
        visible[..start].trim_end().to_string()
    } else { visible }
}

pub(super) fn validated(content: &str, project_path: &str) -> Vec<DeliverableFile> {
    let Some(start) = crate::task_summary::protocol_start(content, false, OPEN) else { return vec![]; };
    let body = &content[start + OPEN.len()..];
    let Some(end) = body.find("</deliverables>") else { return vec![]; };
    let Ok(files) = serde_json::from_str::<Vec<DeliverableFile>>(&body[..end]) else { return vec![]; };
    let Ok(root) = crate::project_files::project_root(project_path) else { return vec![]; };
    let mut result = vec![];
    for mut file in files.into_iter().take(200) {
        let Ok(path) = crate::project_files::normalize_relative_path(&file.path) else { continue; };
        if path.split('/').any(|part| [".git", ".codegraph", crate::brand::PROJECT_DATA_DIRECTORY, "node_modules", "target"].contains(&part.to_ascii_lowercase().as_str())) { continue; }
        let Ok(target) = root.join(&path).canonicalize() else { continue; };
        if !target.starts_with(&root) || !target.is_file() { continue; }
        file.path = path;
        result.push(file);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hides_complete_and_partial_manifest_but_keeps_quoted_examples() {
        assert_eq!(visible_content("Done\n<deliv"), "Done");
        assert_eq!(visible_content("Done\n<deliverables>[]</deliverables>"), "Done");
        let quoted = "```xml\n<deliverables>[]</deliverables>\n```";
        assert_eq!(visible_content(quoted), quoted);
    }
    #[test]
    fn accepts_any_file_kind_only_if_the_file_is_in_workspace() {
        let root = std::env::temp_dir().join(format!("nano-delivery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["poem.md", "convert.js", "poem.docx"] { std::fs::write(root.join(name), "data").unwrap(); }
        let response = "Done\n<deliverables>[{\"path\":\"poem.md\",\"kind\":\"final\"},{\"path\":\"convert.js\",\"kind\":\"final\"},{\"path\":\"poem.docx\",\"kind\":\"intermediate\"},{\"path\":\"missing.docx\",\"kind\":\"final\"},{\"path\":\"../secret.txt\",\"kind\":\"final\"}]</deliverables>";
        let files = validated(response, root.to_str().unwrap());
        assert_eq!(files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), vec!["poem.md", "convert.js", "poem.docx"]);
        assert_eq!(files[1].kind, crate::models::DeliverableKind::Final);
        std::fs::remove_dir_all(root).unwrap();
    }
}
