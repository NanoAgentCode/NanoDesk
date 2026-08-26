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
