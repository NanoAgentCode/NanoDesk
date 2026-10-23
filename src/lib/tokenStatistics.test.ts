import { describe, expect, it } from "vitest";
import { estimateTextTokens, formatTokenStatistics } from "./tokenStatistics";
import type { TokenStatistics } from "../types";

describe("token statistics", () => {
  it("counts Unicode characters rather than UTF-8 bytes or UTF-16 units", () => {
    expect(estimateTextTokens("")).toBe(0);
    expect(estimateTextTokens("abcdefgh")).toBe(2);
    expect(estimateTextTokens("你好ab😀")).toBe(4);
  });
  const stats: TokenStatistics = {
    estimated_output_tokens: 120, input_tokens: null, output_tokens: null,
    generation_ms: 3000, tokens_per_second: 32.5, average_tokens_per_second: 40,
    status: "streaming", model: "test-model", model_config_id: "test-config"
  };
  it("uses ~ for live estimates and preserves zero as a valid server count", () => {
    expect(formatTokenStatistics(stats).label).toBe("已生成 ~120 tokens · ~32.5 tokens/s");
    const done = formatTokenStatistics({ ...stats, status: "completed", input_tokens: 0, output_tokens: 0 });
    expect(done.label).toBe("输出 0 tokens · 平均 ~40.0 tokens/s");
    expect(done.detail).toContain("请求输入 0 tokens");
    expect(done.detail).toContain("服务端");
  });
  it("distinguishes waiting, failed and interrupted output without inventing usage", () => {
    expect(formatTokenStatistics({ ...stats, estimated_output_tokens: 0, tokens_per_second: null }).label).toBe("等待响应…");
    expect(formatTokenStatistics({ ...stats, status: "failed" }).label).toContain("失败 · 已生成 ~120 tokens");
    expect(formatTokenStatistics({ ...stats, status: "interrupted" }).label).toContain("已停止 · 已生成 ~120 tokens");
    expect(formatTokenStatistics({ ...stats, status: "completed", average_tokens_per_second: null }).label).toBe("输出 ~120 tokens · 平均 —");
    expect(formatTokenStatistics(stats).detail).toContain("请求输入未知");
  });
});
