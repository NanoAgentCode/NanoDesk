import { describe, expect, it } from "vitest";
import { collectDeliverables } from "./deliverables";
import type { AgentToolCall, PersistedMessage, RagFile } from "../types";
const message = (id: string, role: PersistedMessage["role"], content = "", conversation_id = "chat") => ({ id, role, content, conversation_id, created_at: "" });
const tool = (path: string, status = "completed", message_id = "write") => ({ name: "write_file", args_json: JSON.stringify({ path }), status, message_id } as AgentToolCall);
describe("conversation deliverables", () => {
  it("groups uploads, successful writes and linked final files, deduplicating paths", () => {
    const messages = [message("upload", "user", "图片附件：\n- pic.png: .nanodesk/uploads/images/pic.png\n需要识别图片文字时，请调用 ocr_image 工具。"), message("write", "assistant"), message("reply", "assistant", "[报告](out/report.md)")];
    const files = [{ id: "f", name: "source.pdf", conversation_id: "chat" } as RagFile];
    expect(collectDeliverables("chat", messages, files, [tool("out/draft.md"), tool("out/report.md"), tool("out\\report.md")]).map(file => [file.name, file.kind])).toEqual([
      ["source.pdf", "upload"], ["pic.png", "upload"], ["draft.md", "intermediate"], ["report.md", "final"],
    ]);
  });
  it("excludes failed, pending, foreign and merely mentioned files", () => {
    expect(collectDeliverables("chat", [message("write", "assistant", "[missing](missing.md)"), message("other", "assistant", "", "elsewhere")], [], [tool("failed.md", "failed"), tool("pending.md", "pending"), tool("foreign.md", "completed", "other"), { ...tool("broken.md"), args_json: "{" }])).toEqual([]);
  });
  it("includes linked existing files produced by commands, without including unrelated project files", () => {
    expect(collectDeliverables("chat", [message("reply", "assistant", "[结果](D:/project/out/result.pdf)")], [], [], [{ path: "out/result.pdf", is_dir: false }, { path: "other.txt", is_dir: false }], "D:/project")).toEqual([
      { id: "out/result.pdf", path: "out/result.pdf", name: "result.pdf", kind: "final" },
    ]);
  });
});
