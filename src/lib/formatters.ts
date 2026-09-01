import type { ProjectFileEntry } from "../types";

export function formatBytes(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatJson(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

export function formatMcpTransportLabel(transport: string) {
  if (transport === "streamable_http") return "Streamable HTTP";
  if (transport === "sse") return "SSE";
  return "stdio";
}

export function isSupportedRagFile(name: string) {
  return /\.(txt|md|markdown|json|csv|tsv|log|js|jsx|ts|tsx|rs|py|java|go|yaml|yml|toml|html|css|xml|pdf|doc|docx|xlsx|pptx)$/i.test(name);
}

export function formatDateTime(dateStr: string | null | undefined): string {
  if (!dateStr) return "";
  try {
    const d = new Date(dateStr);
    if (isNaN(d.getTime())) return dateStr;
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return dateStr;
  }
}

export function formatProjectFileTree(files: ProjectFileEntry[]) {
  return files
    .map((file) => {
      const depth = Math.max(0, file.path.split("/").length - 1);
      const indent = "  ".repeat(depth);
      const name = file.path.split("/").pop() || file.path;
      const suffix = file.is_dir ? "/" : file.size != null ? ` (${formatBytes(file.size)})` : "";
      return `${indent}- ${name}${suffix}`;
    })
    .join("\n");
}

export function buildRuntimeContext() {
  const now = new Date();
  const dateTimeFormatter = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    weekday: "long",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
    timeZoneName: "short"
  });

  return [
    "运行上下文：",
    `- 当前本地日期时间：${dateTimeFormatter.format(now)}`,
    `- 当前 ISO 时间：${now.toISOString()}`,
    "- 用户询问当前日期、时间、今天、明天、昨天或相对日期时，必须以本运行上下文为准。"
  ].join("\n");
}

export function estimateTokens(content: string): number {
  const chineseChars = content.match(/[\u4e00-\u9fa5]/g) || [];
  const englishWords = content.replace(/[\u4e00-\u9fa5]/g, ' ').split(/\s+/).filter(Boolean);
  return chineseChars.length + Math.ceil(englishWords.length * 1.3);
}

export function formatErrorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
