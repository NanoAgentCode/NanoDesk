import { describe, expect, it } from "vitest";
import { createChatStreamAccumulator } from "./chatStreamAccumulator";

describe("chat stream accumulator", () => {
  it("collects the complete response independently from UI visibility", () => {
    const stream = createChatStreamAccumulator("request-1");
    stream.accept({ type: "delta", request_id: "request-1", content: "前半" });
    // UI can switch conversations here; collection remains request-scoped.
    stream.accept({ type: "delta", request_id: "request-1", content: "后半" });
    stream.accept({ type: "reasoning_delta", request_id: "request-1", content: "思考" });
    stream.accept({ type: "done", request_id: "request-1" });

    expect(stream.snapshot()).toEqual({
      content: "前半后半",
      reasoning: "思考",
      error: null,
      done: true
    });
  });

  it("ignores events belonging to concurrent requests", () => {
    const stream = createChatStreamAccumulator("request-1");
    expect(stream.accept({ type: "delta", request_id: "request-2", content: "wrong" })).toBeNull();
    expect(stream.snapshot().content).toBe("");
  });
});
