import type { TokenStatistics } from "../types";

/** Matches the existing usage-page heuristic; this is not a model tokenizer. */
export function estimateTextTokens(text: string): number {
  let ascii = 0, other = 0;
  for (const ch of text) {
    if (ch.codePointAt(0)! <= 127) ascii++;
    else other++;
  }
  return Math.ceil(ascii / 4) + other;
}

export function formatTokenStatistics(stats: TokenStatistics) {
  const live = stats.status === "streaming";
  const count = !live && stats.output_tokens != null ? `${stats.output_tokens}` : `~${stats.estimated_output_tokens}`;
  const rate = live ? stats.tokens_per_second : stats.average_tokens_per_second;
  const speed = rate != null ? `~${rate.toFixed(1)} tokens/s` : "—";
  const prefix = stats.status === "interrupted" ? "已停止 · 已生成"
    : stats.status === "failed" ? "失败 · 已生成" : live ? "已生成" : "输出";
  const label = live && stats.estimated_output_tokens === 0 ? "等待响应…"
    : `${prefix} ${count} tokens · ${live ? "" : "平均 "}${speed}`;
  const input = stats.input_tokens != null ? `请求输入 ${stats.input_tokens} tokens（服务端）` : "请求输入未知";
  return { label, detail: [
    `模型：${stats.model}`, input,
    stats.output_tokens != null ? `输出 ${stats.output_tokens} tokens（服务端，可能包含隐藏推理）` : "输出为文本估算，非计费用量",
    "请求输入包含服务端统计的历史、系统提示与附件上下文。",
    "速度按正文及可见思考的文本估算；实时取最近 2 秒，平均排除首字等待、请求间等待和末尾用量等待。",
    "估算规则：ASCII 字符数 / 4 向上取整 + 非 ASCII 字符数。"
  ].join("\n") };
}
