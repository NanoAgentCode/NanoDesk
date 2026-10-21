import { beforeEach, describe, expect, it, vi } from "vitest";
import { processAttachmentBatch, type AttachmentBatchContext } from "./attachmentBatch";

const api = vi.hoisted(() => ({ readAbsoluteFile: vi.fn(), extractUploadedFile: vi.fn(), listRagFiles: vi.fn() }));
vi.mock("../../api", () => api);

function fixture() {
  return {
    chatVisible: true, busy: false, attachmentDecisionPending: false,
    attachmentBusyRef: { current: false as boolean }, attachmentLockedRef: { current: false },
    attachmentScopeRef: { current: "chat:/project" },
    setUploadingAttachment: vi.fn(), setNotice: vi.fn(),
    conv: {
      activeConversationId: "chat", resolveConversationModelId: vi.fn(() => "model"),
      ensureConversation: vi.fn(async () => "chat"), getConversationProjectHint: vi.fn(() => null)
    },
    rag: { ragFiles: [], setRagFiles: vi.fn(), setIndexingRagFileName: vi.fn(), setIsRagDragging: vi.fn() },
    attachments: { handleImageFiles: vi.fn(async () => 1), attachDroppedImagePaths: vi.fn(async () => 1) },
    speech: { isBusy: vi.fn(() => false), transcribeSources: vi.fn(async () => ({ count: 1, errors: [] as string[], cancelled: false })) },
    getAttachmentProjectPath: () => "/project",
    prepareDocumentRef: { current: vi.fn().mockResolvedValue({ id: "rag" }) }
  } satisfies AttachmentBatchContext;
}
const paths = (...names: string[]) => names.map(path => ({ kind: "path" as const, path }));

beforeEach(() => {
  vi.clearAllMocks();
  api.readAbsoluteFile.mockResolvedValue({ content: "document", size: 8 });
  api.listRagFiles.mockResolvedValue([{ id: "rag" }]);
});

describe("chat attachment batch", () => {
  it("processes images, audio and documents and refreshes only the originating conversation", async () => {
    const ctx = fixture();
    await processAttachmentBatch(ctx, paths("report.md", "voice.wav", "photo.png"));
    expect(ctx.attachments.attachDroppedImagePaths).toHaveBeenCalledWith(["photo.png"]);
    expect(ctx.speech.transcribeSources).toHaveBeenCalledWith(paths("voice.wav"));
    expect(ctx.prepareDocumentRef.current).toHaveBeenCalledWith(expect.objectContaining({ conversationId: "chat", modelConfigId: "model" }));
    expect(ctx.rag.setRagFiles).toHaveBeenCalledWith([{ id: "rag" }]);
    expect(ctx.attachmentBusyRef.current).toBe(false);
    expect(ctx.setUploadingAttachment.mock.calls).toEqual([[true], [false]]);
    expect(ctx.rag.setIndexingRagFileName).toHaveBeenLastCalledWith("");
    expect(ctx.rag.setIsRagDragging).toHaveBeenLastCalledWith(false);
  });

  it("stops before indexing if the user changes conversation during file reading", async () => {
    const ctx = fixture();
    api.readAbsoluteFile.mockImplementationOnce(async () => {
      ctx.attachmentScopeRef.current = "other:/project";
      return { content: "stale", size: 5 };
    });
    await processAttachmentBatch(ctx, paths("report.md"));
    expect(ctx.prepareDocumentRef.current).not.toHaveBeenCalled();
    expect(ctx.rag.setRagFiles).not.toHaveBeenCalled();
    expect(ctx.setNotice).not.toHaveBeenCalled();
    expect(ctx.attachmentBusyRef.current).toBe(false);
  });

  it("does not publish files fetched after a conversation switch", async () => {
    const ctx = fixture();
    api.listRagFiles.mockImplementationOnce(async () => {
      ctx.attachmentScopeRef.current = "other:/project";
      return [{ id: "old" }];
    });
    await processAttachmentBatch(ctx, paths("report.md"));
    expect(ctx.rag.setRagFiles).not.toHaveBeenCalled();
  });

  it.each([false, true])("accepts the initial conversation creation before or after its render (rendered=%s)", async (rendered) => {
    const ctx = fixture();
    ctx.conv.activeConversationId = "";
    ctx.attachmentScopeRef.current = ":/project";
    ctx.conv.ensureConversation.mockImplementationOnce(async () => {
      if (rendered) ctx.attachmentScopeRef.current = "chat:/project";
      return "chat";
    });
    await processAttachmentBatch(ctx, paths("report.md"), true);
    expect(ctx.prepareDocumentRef.current).toHaveBeenCalledOnce();
    expect(ctx.rag.setRagFiles).toHaveBeenCalledOnce();
  });

  it("rejects overlapping uploads without releasing the existing operation lock", async () => {
    const ctx = fixture();
    ctx.attachmentBusyRef.current = true;
    await expect(processAttachmentBatch(ctx, paths("report.md"), true)).rejects.toThrow("当前暂不能上传附件");
    expect(ctx.attachmentBusyRef.current).toBe(true);
    expect(ctx.setUploadingAttachment).not.toHaveBeenCalled();
    expect(api.readAbsoluteFile).not.toHaveBeenCalled();
  });

  it("stops the remaining batch after speech cancellation and releases busy state", async () => {
    const ctx = fixture();
    ctx.speech.transcribeSources.mockResolvedValueOnce({ count: 0, errors: [], cancelled: true });
    await expect(processAttachmentBatch(ctx, paths("voice.wav", "report.md"), true)).rejects.toThrow("已取消语音识别");
    expect(api.readAbsoluteFile).not.toHaveBeenCalled();
    expect(ctx.attachmentBusyRef.current).toBe(false);
    expect(ctx.setUploadingAttachment).toHaveBeenLastCalledWith(false);
  });

  it("rejects a strict caller when scope changes while reading and still releases the lock", async () => {
    const ctx = fixture();
    api.readAbsoluteFile.mockImplementationOnce(async () => {
      ctx.attachmentScopeRef.current = "other:/project";
      return { content: "stale", size: 5 };
    });
    await expect(processAttachmentBatch(ctx, paths("report.md"), true)).rejects.toThrow("会话已切换");
    expect(ctx.prepareDocumentRef.current).not.toHaveBeenCalled();
    expect(ctx.attachmentBusyRef.current).toBe(false);
    expect(ctx.setUploadingAttachment).toHaveBeenLastCalledWith(false);
  });

  it("does not index without a selected model", async () => {
    const ctx = fixture();
    ctx.conv.resolveConversationModelId.mockReturnValueOnce("");
    await expect(processAttachmentBatch(ctx, paths("report.md"), true)).rejects.toThrow("保存并选择一个聊天模型");
    expect(ctx.conv.ensureConversation).not.toHaveBeenCalled();
    expect(api.readAbsoluteFile).not.toHaveBeenCalled();
    expect(ctx.attachmentBusyRef.current).toBe(false);
  });

  it("reports file errors to strict callers and always clears indexing status", async () => {
    const ctx = fixture();
    api.readAbsoluteFile.mockRejectedValueOnce(new Error("unreadable"));
    await expect(processAttachmentBatch(ctx, paths("report.md"), true)).rejects.toThrow("unreadable");
    expect(ctx.rag.setRagFiles).not.toHaveBeenCalled();
    expect(ctx.rag.setIndexingRagFileName).toHaveBeenLastCalledWith("");
    expect(ctx.attachmentBusyRef.current).toBe(false);
  });
});
