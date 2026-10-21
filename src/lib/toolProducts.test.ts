import { describe, expect, it } from "vitest";
import { restoreToolProducts } from "./toolProducts";
import type { AgentRunTimeline } from "../types";
const timeline = (metadata: unknown) => ({ tool_calls: [{ id: "tool", name: "execute_command", status: "completed" }], steps: [
  { kind: "generated_files", status: "completed", metadata_json: JSON.stringify(metadata) }
] } as AgentRunTimeline);

describe("persisted command products", () => {
  it("restores products associated with the exact tool after reopening history", () => {
    expect(restoreToolProducts(timeline({ tool_call_id: "tool", files: ["out/poem.docx", "out/poem.docx"] }))[0].generated_files).toEqual(["out/poem.docx"]);
    expect(restoreToolProducts(timeline({ tool_call_id: "other", files: ["foreign.txt"] }))[0].generated_files).toEqual([]);
  });
  it("ignores malformed, failed and out-of-workspace records", () => {
    const value = timeline({ tool_call_id: "tool", files: ["../secret", "C:/secret", "/secret", ".git/config", 7, "poem.docx"] });
    value.steps.push({ ...value.steps[0], metadata_json: "{" });
    value.steps.push({ ...value.steps[0], status: "failed", metadata_json: JSON.stringify({ tool_call_id: "tool", files: ["failed.txt"] }) });
    expect(restoreToolProducts(value)[0].generated_files).toEqual(["poem.docx"]);
  });
});
