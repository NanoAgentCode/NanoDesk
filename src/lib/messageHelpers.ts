export function parseTags(value: string) {
  return value
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
}

export function extractMemoryDraft(content: string) {
  const normalized = content.trim();
  const memoryIntent =
    /(记住|记一下|记到记忆|保存到记忆|加入记忆|更新(?:一下)?(?:我的)?记忆|修改(?:一下)?(?:我的)?记忆|以后记得)/.test(normalized);

  if (!memoryIntent) {
    return null;
  }

  const memoryContent = normalized
    .replace(/^(请|帮我|麻烦你|你)?\s*/, "")
    .replace(/^(记住|记一下|记到记忆|保存到记忆|加入记忆|以后记得)[：:\s]*/i, "")
    .replace(/^更新(?:一下)?(?:我的)?记忆[：:\s]*/i, "")
    .replace(/^修改(?:一下)?(?:我的)?记忆[：:\s]*/i, "")
    .trim();

  if (!memoryContent) {
    return null;
  }

  const title = memoryContent
    .replace(/[。.!！?？\n\r].*$/s, "")
    .slice(0, 24)
    .trim() || "聊天记忆";

  return {
    title,
    content: memoryContent,
    tags: ["chat"],
    enabled: true
  };
}

export function isExplicitProfileInstruction(content: string) {
  const normalized = content.replace(/\s+/g, " ").trim();
  const explicit = /(记住我|请记住我的|以后都|从现在起)|\b(remember that i|please remember my|from now on)\b/i.test(normalized);
  const profileSignal = /(我|我的).*(偏好|喜欢|习惯|默认|身份|角色|工作|使用|项目|环境|语言|格式|语气|简洁|详细)|\b(i|my)\b.*\b(prefer|like|usually|always|role|work|use|project|language|format|tone)\b/i.test(normalized);
  return explicit && profileSignal;
}

export type UserMemoryRoute =
  | { kind: "profile"; memoryDraft: null }
  | { kind: "memory"; memoryDraft: NonNullable<ReturnType<typeof extractMemoryDraft>> }
  | { kind: "auto"; memoryDraft: null };

export function resolveUserMemoryRoute(content: string, memoryContent = content): UserMemoryRoute {
  if (isExplicitProfileInstruction(content)) {
    return { kind: "profile", memoryDraft: null };
  }

  const memoryDraft = extractMemoryDraft(memoryContent);
  return memoryDraft
    ? { kind: "memory", memoryDraft }
    : { kind: "auto", memoryDraft: null };
}

export interface ParsedToolCall {
  name: string;
  args: Record<string, string>;
  raw: string;
}

export interface ParsedToolResult {
  name: string;
  status: "success" | "failed" | "rejected" | "unknown";
  summary: string;
  detail: string;
}

export function parseToolCall(content: string): ParsedToolCall | null {
  if (!content) return null;
  const match = content.match(/<tool_call\s+name="([^"]+)">([\s\S]*?)<\/tool_call>/);
  if (!match) return null;

  const name = match[1];
  const body = match[2];
  const args: Record<string, string> = {};

  const tagRegex = /<([^>]+)>([\s\S]*?)<\/\1>/g;
  let tagMatch;
  while ((tagMatch = tagRegex.exec(body)) !== null) {
    args[tagMatch[1]] = tagMatch[2].trim();
  }

  return { name, args, raw: match[0] };
}

export function parseToolResult(content: string): ParsedToolResult | null {
  if (!content) return null;
  const match = content.match(/^\[工具执行结果: ([^\]]+)\]\s*([\s\S]*)$/);
  if (!match) return null;

  const name = match[1].trim();
  const body = match[2].trim();
  if (body.startsWith("执行失败")) {
    return {
      name,
      status: "failed",
      summary: "执行失败",
      detail: body.replace(/^执行失败[:：]?\s*/, "").trim() || body
    };
  }
  if (body.startsWith("执行结果如下")) {
    return {
      name,
      status: "success",
      summary: "执行完成",
      detail: body.replace(/^执行结果如下[:：]?\s*/, "").trim() || body
    };
  }
  if (body.includes("用户拒绝")) {
    return {
      name,
      status: "rejected",
      summary: "用户拒绝",
      detail: body
    };
  }

  return {
    name,
    status: "unknown",
    summary: "工具结果",
    detail: body
  };
}

export function parseClarificationRequest(content: string): AgentClarificationRequest | null {
  if (!content) return null;
  const match = content.match(/<clarification>([\s\S]*?)<\/clarification>/);
  if (!match) return null;

  try {
    const value = JSON.parse(match[1].trim()) as Partial<AgentClarificationRequest>;
    if (!Array.isArray(value.questions) || value.questions.length < 1 || value.questions.length > 3) {
      return null;
    }
    const questions = value.questions.map((question) => {
      if (
        !question ||
        typeof question.id !== "string" || !question.id.trim() ||
        typeof question.prompt !== "string" || !question.prompt.trim() ||
        !Array.isArray(question.options) || question.options.length < 2 || question.options.length > 5
      ) {
        throw new Error("invalid clarification question");
      }
      const ids = new Set<string>();
      const options = question.options.map((option) => {
        if (
          !option ||
          typeof option.id !== "string" || !option.id.trim() || ids.has(option.id) ||
          typeof option.label !== "string" || !option.label.trim()
        ) {
          throw new Error("invalid clarification option");
        }
        ids.add(option.id);
        return {
          id: option.id.trim(),
          label: option.label.trim(),
          description: typeof option.description === "string" && option.description.trim()
            ? option.description.trim()
            : null,
          recommended: option.recommended === true
        };
      });
      return {
        id: question.id.trim(),
        prompt: question.prompt.trim(),
        options,
        allow_custom: question.allow_custom !== false
      };
    });
    if (new Set(questions.map((question) => question.id)).size !== questions.length) return null;
    return { questions };
  } catch {
    return null;
  }
}

export function formatClarificationAnswerMessage(
  messageId: string,
  request: AgentClarificationRequest,
  answers: AgentClarificationAnswer[],
  automatic: boolean
) {
  const lines = answers.map((answer) => {
    const question = request.questions.find((item) => item.id === answer.question_id);
    const option = question?.options.find((item) => item.id === answer.option_id);
    const response = answer.skipped
      ? "已跳过"
      : answer.custom_text?.trim() || option?.label || "未回答";
    return `- ${question?.prompt || answer.question_id}：${response}`;
  });
  return `[澄清回答: ${messageId}]${automatic ? "（自动选择）" : ""}\n${lines.join("\n")}`;
}

const taskPlanStatuses = new Set<AgentTaskPlanStepStatus>([
  "pending", "in_progress", "completed", "blocked", "skipped"
]);

export function parseTaskPlan(content: string): AgentTaskPlan | null {
  if (!content) return null;
  const match = content.match(/<task_plan>([\s\S]*?)<\/task_plan>/);
  if (!match) return null;

  try {
    const value = JSON.parse(match[1].trim()) as Partial<AgentTaskPlan>;
    if (
      typeof value.goal !== "string" || !value.goal.trim() ||
      !Array.isArray(value.steps) || value.steps.length < 2 || value.steps.length > 12
    ) return null;

    const ids = new Set<string>();
    let activeCount = 0;
    const steps = value.steps.map((step) => {
      if (
        !step || typeof step.id !== "string" || !step.id.trim() || ids.has(step.id.trim()) ||
        typeof step.title !== "string" || !step.title.trim() ||
        typeof step.status !== "string" || !taskPlanStatuses.has(step.status as AgentTaskPlanStepStatus)
      ) throw new Error("invalid task plan step");
      ids.add(step.id.trim());
      if (step.status === "in_progress") activeCount += 1;
      return {
        id: step.id.trim(),
        title: step.title.trim(),
        status: step.status as AgentTaskPlanStepStatus,
        detail: typeof step.detail === "string" && step.detail.trim() ? step.detail.trim() : null
      };
    });
    if (activeCount > 1) return null;
    return { goal: value.goal.trim(), steps };
  } catch {
    return null;
  }
}

export function stripTaskPlan(content: string) {
  return content.replace(/<task_plan>[\s\S]*?<\/task_plan>/g, "").trim();
}

export function stripClarificationRequest(content: string) {
  return content.replace(/\s*<clarification>[\s\S]*?<\/clarification>\s*/g, "\n\n").trim();
}

export function buildAutomaticClarificationAnswers(request: AgentClarificationRequest) {
  return request.questions.map((question): AgentClarificationAnswer => ({
    question_id: question.id,
    option_id: (question.options.find((option) => option.recommended) || question.options[0]).id
  }));
}

export function findPendingClarification(messages: PersistedMessage[]) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role !== "assistant") continue;
    const request = parseClarificationRequest(message.content);
    if (!request) continue;
    const responsePrefix = `[澄清回答: ${message.id}]`;
    const answered = messages.slice(index + 1).some((item) =>
      item.role === "user" && item.content.startsWith(responsePrefix)
    );
    if (!answered) return { messageId: message.id, request };
  }
  return null;
}
import type {
  AgentClarificationAnswer,
  AgentClarificationRequest,
  AgentTaskPlan,
  AgentTaskPlanStepStatus,
  PersistedMessage
} from "../types";
