import { describe, expect, it, vi } from "vitest";
import type { RagFile } from "../types";
import { createDocumentPreparer } from "./documentPreparation";
describe("shared document preparation", () => {
  it("reuses a prepared attachment across followups but invalidates removal, model and content changes", async () => {
    const file = { id: "file" } as RagFile;
    const index = vi.fn().mockResolvedValue(file);
    const prepare = createDocumentPreparer(index);
    const request = { conversationId: "chat", modelConfigId: "model", source: { name: "a.txt", content: "hello" }, existingFiles: [file] };
    await prepare(request); await prepare(request);
    expect(index).toHaveBeenCalledTimes(1);
    await prepare({ ...request, existingFiles: [] });
    await prepare({ ...request, modelConfigId: "other" });
    request.source.content = "changed";
    await prepare(request);
    await prepare({ ...request, conversationId: "other" });
    expect(index).toHaveBeenCalledTimes(5);
  });
  it("shares in-flight work and retries failed preparation", async () => {
    let reject!: (error: Error) => void;
    const index = vi.fn().mockReturnValueOnce(new Promise((_, fail) => { reject = fail; })).mockResolvedValue({ id: "ok" });
    const prepare = createDocumentPreparer(index);
    const request = { conversationId: "chat", modelConfigId: "model", source: { name: "a", content: "x" }, existingFiles: [] };
    const first = prepare(request), second = prepare(request);
    const failures = Promise.allSettled([first, second]);
    reject(new Error("offline"));
    expect((await failures).every(result => result.status === "rejected")).toBe(true);
    expect(index).toHaveBeenCalledOnce();
    await expect(prepare(request)).resolves.toEqual({ id: "ok" });
    expect(index).toHaveBeenCalledTimes(2);
  });
  it("invalidates changed model configuration and failed background preparation", async () => {
    const file = { id: "file" } as RagFile;
    const index = vi.fn().mockResolvedValue(file);
    const prepare = createDocumentPreparer(index);
    const request = { conversationId: "chat", modelConfigId: "model", modelRevision: "v1", source: { name: "a", content: "x" }, existingFiles: [file] };
    await prepare(request);
    await prepare({ ...request, modelRevision: "v2" });
    await prepare({ ...request, modelRevision: "v2", existingFiles: [{ ...file, preparation: { phase: "failed", completed: 0, total: 1, model_id: "model" } }] });
    expect(index).toHaveBeenCalledTimes(3);
  });
});
