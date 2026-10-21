import { describe, expect, it, vi } from "vitest";
vi.mock("./imageAttachments", async importOriginal => ({ ...await importOriginal<typeof import("./imageAttachments")>(), fileToDataUrl: async () => "data:text/plain;base64,aGVsbG8=" }));
import { prepareAssistantClipboard, restoreAssistantClipboardFile } from "./assistantClipboard";

describe("assistant clipboard", () => {
  it("uses native copied paths, filters unsupported formats and avoids duplicate browser files", async () => {
    expect(await prepareAssistantClipboard([new File(["hello"], "notes.txt")], async () => ["D:/notes.txt", "D:/app.exe"])).toEqual({ paths: ["D:/notes.txt"], files: [], unsupported: 1 });
  });
  it("encodes browser files and restores their bytes for the existing uploader", async () => {
    const result = await prepareAssistantClipboard([new File(["hello"], "notes.txt", { type: "text/plain" })], async () => []);
    expect(result.paths).toEqual([]);
    const file = restoreAssistantClipboardFile(result.files[0]);
    expect(file.name).toBe("notes.txt");
    expect(await file.text()).toBe("hello");
  });
  it("leaves text-only clipboard content without attachments", async () => {
    expect(await prepareAssistantClipboard([], async () => [])).toEqual({ paths: [], files: [], unsupported: 0 });
  });
});
