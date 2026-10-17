export type NoticeType = "info" | "success" | "warning" | "error";

/** Raw errors stay in runtime records; normal UI surfaces receive only this summary. */
export function formatUserFacingError(error: unknown, fallback = "操作失败，请重试。") {
  const text = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  if (text.includes("正在等待当前工具结束后暂停")) return "正在等待工具完成后暂停，请稍候。";
  if (/^[\u4e00-\u9fff]{1,24}(?:失败|超时)，请(?:稍后)?重试。$/.test(text)) return text;
  if (/任务预算|budget.*(?:exhaust|exceed)/i.test(text)) return "任务预算不足，请调整预算后继续。";
  if (/command timed out|命令.*超时/i.test(text)) return "命令执行超时，请稍后重试。";
  if (/command failed|powershell syntax error|命令.*失败|parsererror|unexpectedtoken|categoryinfo|fullyqualifiederrorid|stdout:|stderr:/i.test(text)) {
    return "命令执行失败，请重试。";
  }
  if (/工具|tool.*(?:fail|error)|mcp/i.test(text)) return "工具调用失败，请重试。";
  if (/network|failed to fetch|fetch failed|connection|网络|连接.*失败|连接.*超时/i.test(text)) {
    return "网络连接失败，请稍后重试。";
  }
  const operation = text.match(/^([\u4e00-\u9fffA-Za-z0-9 、_-]{1,24}失败)[：:]/)?.[1];
  return operation ? `${operation}，请重试。` : fallback;
}

export function getNoticeData(content: string): { content: string; type: NoticeType } {
  // Failure takes precedence even when stdout happens to contain "success".
  if (/失败|错误|error|failed|invalid|exception|无法|不可用|超时|denied|unavailable|timeout|HTTP\s*[45]\d\d|工具策略拒绝|\[object Object\]/i.test(content)) {
    return { content: formatUserFacingError(content), type: "error" };
  }
  if (/成功|success|已保存|已删除|已恢复|已清空|已同步/i.test(content)) {
    return { content, type: "success" };
  }
  if (/警告|warning|请先|不能为空|限制|注意|缺少|未检测到/i.test(content)) {
    return { content, type: "warning" };
  }
  // Do not display unclassified stack traces, response bodies or long backend output.
  if (/\n|\r|^\s*[{[]|\bat \S+\s*\(/.test(content) || content.length > 200 || !/[\u4e00-\u9fff]/.test(content)) {
    return { content: formatUserFacingError(content), type: "error" };
  }
  return { content, type: "info" };
}
