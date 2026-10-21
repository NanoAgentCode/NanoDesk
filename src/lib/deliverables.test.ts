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
  it("collects recorded command products without a project file tree and promotes linked outputs", () => {
    const command = { ...tool(""), name: "execute_command", generated_files: ["poem.docx", "draft.txt"] };
    expect(collectDeliverables("chat", [message("write", "assistant"), message("reply", "assistant", "[Word](poem.docx)")], [], [command]).map(file => [file.path, file.kind])).toEqual([
      ["poem.docx", "final"], ["draft.txt", "intermediate"]
    ]);
    expect(collectDeliverables("chat", [message("write", "assistant")], [], [{ ...command, status: "failed" }])).toEqual([]);
  });
  it("keeps generated scripts intermediate even when linked alongside the delivered document", () => {
    expect(collectDeliverables("chat", [message("write", "assistant"), { ...message("reply", "assistant", "[脚本](convert.js) [文档](poem.docx)"), metadata: { deliverables: [{ path: "convert.js", kind: "intermediate" as const }, { path: "poem.docx", kind: "final" as const }] } }], [], [tool("convert.js"), { ...tool(""), name: "execute_command", generated_files: ["poem.docx"] }]).map(file => [file.path, file.kind])).toEqual([
      ["convert.js", "intermediate"], ["poem.docx", "final"]
    ]);
  });
  it("keeps deliveries across many rounds and does not require links or a non-script extension", () => {
    const first = { ...message("first", "assistant", "诗已写入"), metadata: { deliverables: [{ path: "poem.md", kind: "final" as const }] } };
    const unrelated = Array.from({ length: 50 }, (_, i) => message(`round-${i}`, "assistant", "继续讨论"));
    const converted = { ...message("converted", "assistant", "转换完成"), metadata: { deliverables: [
      { path: "poem.md", kind: "intermediate" as const }, { path: "convert.js", kind: "intermediate" as const }, { path: "poem.docx", kind: "final" as const }
    ] } };
    const messages = [first, ...unrelated, converted];
    const restored = JSON.parse(JSON.stringify(messages));
    expect(collectDeliverables("chat", restored, [], []).map(file => [file.path, file.kind])).toEqual([
      ["poem.md", "final"], ["convert.js", "intermediate"], ["poem.docx", "final"]
    ]);
    const scriptDelivery = { ...message("script", "assistant"), metadata: { deliverables: [{ path: "convert.js", kind: "final" as const }] } };
    expect(collectDeliverables("chat", [...messages, scriptDelivery], [], []).find(file => file.path === "convert.js")?.kind).toBe("final");
  });
  it("does not infer delivery from links for new responses or accept interrupted manifests", () => {
    const reply = { ...message("reply", "assistant", "[链接](draft.md)"), metadata: { deliverables: [] } };
    const interrupted = { ...message("interrupted", "assistant"), metadata: { generation_status: "interrupted", deliverables: [{ path: "unfinished.md", kind: "final" as const }] } };
    expect(collectDeliverables("chat", [message("write", "assistant"), reply, interrupted], [], [tool("draft.md")]).map(file => file.kind)).toEqual(["intermediate"]);
  });
  it("keeps temporary markdown and scripts intermediate when the original goal is a Word document", () => {
    const reply = { ...message("reply", "assistant", "文档完成"), metadata: { deliverables: [
      { path: "poem.md", kind: "intermediate" as const }, { path: "convert.js", kind: "intermediate" as const }, { path: "poem.docx", kind: "final" as const }
    ] } };
    expect(collectDeliverables("chat", [message("write", "assistant"), reply], [], [tool("poem.md"), tool("convert.js"), { ...tool(""), name: "execute_command", generated_files: ["poem.docx"] }]).map(file => [file.path, file.kind])).toEqual([
      ["poem.md", "intermediate"], ["convert.js", "intermediate"], ["poem.docx", "final"]
    ]);
  });
});
