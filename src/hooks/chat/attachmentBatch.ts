import type { MutableRefObject } from "react";
import { extractUploadedFile, listRagFiles, readAbsoluteFile } from "../../api";
import { fileToDataUrl } from "../../lib/imageAttachments";
import { attachmentName, partitionAttachments, formatAttachmentUploadResult, type AttachmentSource } from "../../lib/attachmentUploads";
import type { createDocumentPreparer } from "../../lib/documentPreparation";
import type { UseConversationsReturn } from "../useConversations";
import type { UseRagFilesReturn } from "../useRagFiles";
import type { useChatAttachments } from "../useChatAttachments";
import type { UseSpeechInputReturn } from "../useSpeechInput";

export interface AttachmentBatchContext {
  chatVisible: boolean;
  busy: boolean;
  attachmentDecisionPending: boolean;
  attachmentBusyRef: MutableRefObject<boolean>;
  attachmentLockedRef: MutableRefObject<boolean>;
  attachmentScopeRef: MutableRefObject<string>;
  setUploadingAttachment: (value: boolean) => void;
  setNotice: (message: string) => void;
  conv: Pick<UseConversationsReturn, "activeConversationId" | "resolveConversationModelId" | "ensureConversation" | "getConversationProjectHint">;
  rag: Pick<UseRagFilesReturn, "ragFiles" | "setRagFiles" | "setIndexingRagFileName" | "setIsRagDragging">;
  attachments: Pick<ReturnType<typeof useChatAttachments>, "handleImageFiles" | "attachDroppedImagePaths">;
  speech: Pick<UseSpeechInputReturn, "isBusy" | "transcribeSources">;
  getAttachmentProjectPath: () => string;
  prepareDocumentRef: MutableRefObject<ReturnType<typeof createDocumentPreparer>>;
}

export async function processAttachmentBatch(
  context: AttachmentBatchContext, sources: AttachmentSource[], strict = false
) {
  const { chatVisible, busy, attachmentDecisionPending, attachmentBusyRef, attachmentLockedRef,
    attachmentScopeRef, setUploadingAttachment, setNotice, conv, rag, attachments, speech,
    getAttachmentProjectPath, prepareDocumentRef } = context;
  if (!sources.length) return;
  if (!chatVisible) { rag.setIsRagDragging(false); if (strict) throw new Error("请先切换到聊天页面。"); return; }
  if (busy || attachmentDecisionPending || attachmentBusyRef.current || speech.isBusy()) {
    rag.setIsRagDragging(false);
    setNotice("当前暂不能上传附件，请先完成正在进行的操作。");
    if (strict) throw new Error("当前暂不能上传附件，请先完成正在进行的操作。");
    return;
  }
  attachmentBusyRef.current = true;
  setUploadingAttachment(true);
  let scopeKey = attachmentScopeRef.current;
  // Creating a first conversation can render before or after ensureConversation resolves.
  let transitionScope: string | null = null;
  const active = () => {
    if (attachmentLockedRef.current) { if (strict) throw new Error("附件处理已停止，请检查当前会话。"); return false; }
    if (attachmentScopeRef.current === scopeKey) { transitionScope = null; return true; }
    const matches = transitionScope !== null && attachmentScopeRef.current === transitionScope;
    if (strict && !matches) throw new Error("会话已切换，附件处理已停止。");
    return matches;
  };
  const groups = partitionAttachments(sources);
  const result = { image: 0, audio: 0, document: 0, errors: [] as string[], unsupported: groups.unsupported.map(attachmentName) };
  try {
    for (const source of groups.image) {
      if (!active()) return;
      try {
        const count = source.kind === "file"
          ? await attachments.handleImageFiles([source.file])
          : await attachments.attachDroppedImagePaths([source.path]);
        result.image += count;
        if (!count && active()) result.errors.push(`${attachmentName(source)}：图片添加失败`);
      } catch (error) { result.errors.push(`${attachmentName(source)}：${String(error)}`); }
    }
    if (!active()) return;
    if (groups.audio.length) {
      const audio = await speech.transcribeSources(groups.audio);
      result.audio = audio.count;
      result.errors.push(...audio.errors);
      if (audio.cancelled) {
        if (active()) setNotice("已取消本次附件中的语音识别。");
        if (strict) throw new Error("已取消语音识别。");
        return;
      }
    }
    if (!active()) return;
    if (groups.document.length) {
      const modelConfigId = conv.resolveConversationModelId(conv.activeConversationId);
      if (!modelConfigId) {
        result.errors.push("文档索引：请先保存并选择一个聊天模型配置");
      } else {
        const hadConversation = Boolean(conv.activeConversationId);
        const uploadProjectPath = getAttachmentProjectPath();
        const conversationId = await conv.ensureConversation(conv.getConversationProjectHint());
        if (!hadConversation) {
          transitionScope = scopeKey;
          scopeKey = `${conversationId}:${uploadProjectPath}`;
        }
        if (!active()) return;
        for (const source of groups.document) {
          if (!active()) return;
          const name = attachmentName(source);
          rag.setIndexingRagFileName(name);
          try {
            const extracted = source.kind === "file"
              ? await extractUploadedFile({ name, content_base64: await fileToDataUrl(source.file) })
              : await readAbsoluteFile(source.path);
            if (!active()) return;
            await prepareDocumentRef.current({
              conversationId, source: { name, content: extracted.content }, modelConfigId,
              mime: source.kind === "file" ? source.file.type || "text/plain" : "text/plain",
              size: extracted.size, existingFiles: rag.ragFiles
            });
            result.document++;
          } catch (error) { result.errors.push(`${name}：${String(error)}`); }
        }
        if (active() && result.document) {
          const nextFiles = await listRagFiles(conversationId);
          if (active()) rag.setRagFiles(nextFiles);
        }
      }
    }
    if (active()) setNotice(formatAttachmentUploadResult(result));
    if (strict && (result.errors.length || result.unsupported.length)) throw new Error(formatAttachmentUploadResult(result));
  } catch (error) {
    if (active()) setNotice(`附件处理失败：${String(error)}`);
    if (strict) throw error;
  } finally {
    attachmentBusyRef.current = false;
    setUploadingAttachment(false);
    rag.setIndexingRagFileName("");
    rag.setIsRagDragging(false);
  }
}
