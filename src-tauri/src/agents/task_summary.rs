//! Hide legacy summary blocks when displaying historical model output.
pub const OPEN: &str = "<task_summary>";

/// Hide complete and partially streamed protocol blocks, including split opening tags.
pub fn visible_content(content: &str) -> String {
    if let Some(start) = protocol_start(content, true, OPEN) {
        return content[..start].trim_end().to_string();
    }
    content.to_string()
}

// A standalone protocol line outside code fences; quoted examples remain ordinary text.
pub(crate) fn protocol_start(content: &str, partial: bool, open: &str) -> Option<usize> {
    let mut offset = 0;
    let mut fence: Option<&str> = None;
    for line in content.split_inclusive('\n') {
        let text = line.trim_start();
        if text.starts_with("```") || text.starts_with("~~~") {
            let marker = &text[..3];
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
        } else if fence.is_none()
            && (text.starts_with(open) || (partial && !text.is_empty() && open.starts_with(text)))
        {
            return Some(offset + line.len() - text.len());
        }
        offset += line.len();
    }
    None
}
