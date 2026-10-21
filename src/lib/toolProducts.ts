import type { AgentRunTimeline } from "../types";

/** Restore command products from durable run steps, without guessing paths from stdout. */
export function restoreToolProducts(timeline: AgentRunTimeline) {
  const files = new Map<string, string[]>();
  for (const step of timeline.steps) {
    if (step.kind !== "generated_files" || step.status !== "completed" || !step.metadata_json) continue;
    try {
      const data = JSON.parse(step.metadata_json);
      if (typeof data.tool_call_id !== "string" || !Array.isArray(data.files)) continue;
      const paths = data.files.filter((path: unknown): path is string => typeof path === "string"
        && path.length > 0 && !/^(?:[a-z]:|[/\\])/i.test(path)
        && !path.replace(/\\/g, "/").split("/").some(part => ["..", ".git", ".codegraph", ".nanodesk", "node_modules", "target"].includes(part)));
      files.set(data.tool_call_id, [...new Set([...(files.get(data.tool_call_id) || []), ...paths])]);
    } catch { /* A malformed historical step must not hide other tool records. */ }
  }
  return timeline.tool_calls.map(tool => ({ ...tool, generated_files: files.get(tool.id) || [] }));
}
