import type { PersistedMessage } from "../types";

export const SUMMARY_OUTPUT_TOKENS = 1200;

export function buildSummaryPrompt(messages: PersistedMessage[]): string {
  const transcript = messages
    .map((message, index) => `[${index + 1}] ${roleLabel(message.role)}: ${message.content}`)
    .join("\n\n");

  return [
    "你是对话状态压缩器。请把以下历史整理为可直接交接给后续模型的结构化状态摘要。",
    "必须忠实于原文，不得补充未发生的事实。严格按照消息编号和发生顺序理解事件；如果后续消息修改、撤销或否定了早先决定，以后续状态为准，同时简要保留变更关系。",
    "重点抽取任务、已完成工作、当前状态、未完成事项及其先后顺序和依赖关系。保留关键路径、命令、错误、约束、用户偏好和明确决定。",
    "一个聊天以一条主任务为主。只有用户明确提出目标才识别任务；普通问答或目标不明时写未明确，不得虚构目标或进度。",
    "临时插入问题不覆盖主任务，记录插入问题及返回主任务的位置。只有用户明确取消或替换目标才切换主线，歧义保留为待确认。",
    "区分用户已确认要求与助手建议，保留先设计/暂不实施等授权边界。已完成必须附执行结果或用户确认依据；计划、尝试、成功、验证通过不可混淆，不输出进度百分比。",
    "连续压缩时继承仍有效的主任务、约束、未完成事项和恢复位置；新决定覆盖旧决定，明确失效内容，避免重新执行。",
    "请严格使用以下 Markdown 结构：",
    "## 任务与目标",
    "## 已完成（按发生顺序）",
    "## 当前状态",
    "## 待办顺序与依赖",
    "## 关键决定、约束与重要事实",
    "## 最近交接点",
    "## 临时插入问题与主任务恢复位置",
    "若某节无内容，写“无”。不要输出结构之外的解释。",
    "",
    transcript
  ].join("\n");
}

function roleLabel(role: PersistedMessage["role"]): string {
  if (role === "user") return "用户";
  if (role === "assistant") return "助手";
  return "系统摘要";
}
