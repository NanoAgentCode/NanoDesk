import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { listConversations } from "../api/conversations";
import { restoreAssistantVisibility, subscribeAssistantVisibility } from "../api/assistant";
import { ASSISTANT_REPLY_EVENT, ASSISTANT_REQUEST_EVENT, validateAssistantHandoff, type AssistantRequest } from "../lib/assistantMode";
import type { UseChatReturn } from "./useChat";
import type { UseProjectsReturn } from "./useProjects";
import type { UseModelReturn } from "./useModel";
import { restoreAssistantClipboardFile } from "../lib/assistantClipboard";

function requireAssistantModel(model: UseModelReturn, text: string, savedId: string) {
  const routedId = model.routing.enabled ? model.routing.resolve(text)?.modelId : null;
  const id = routedId || (model.routing.enabled ? savedId : model.routing.fixedModelIds.includes(savedId) ? savedId : model.routing.fixedModelIds[0]);
  if (!id || !model.models.some((item) => item.id === id)) throw new Error("请先在主窗口配置并选择聊天模型。");
}

export function useAssistantHandoff(chat: UseChatReturn, projects: UseProjectsReturn, model: UseModelReturn, openChat: () => void, notice: (text: string) => void) {
  const current = useRef({ chat, projects, model, openChat, notice });
  current.current = { chat, projects, model, openChat, notice };
  const processing = useRef(false);
  const [pending, setPending] = useState<(AssistantRequest & { targetId: string; attachmentsReady?: boolean }) | null>(null);

  useEffect(() => {
    const stopVisibility = subscribeAssistantVisibility(() => {});
    void restoreAssistantVisibility().catch((error) => current.current.notice(`恢复桌面助手失败：${String(error)}`));
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<AssistantRequest>(ASSISTANT_REQUEST_EVENT, ({ payload: request }) => {
      void (async () => {
        if (processing.current) {
          await emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id, error: "正在接收上一条消息，请稍后重试。" }); return;
        }
        processing.current = true;
        try {
          const { chat: state, projects: projectState, model: models } = current.current;
          validateAssistantHandoff(state.busy || state.uploadingAttachment || state.speech.isBusy(), state.chatInput, state.pendingImageAttachments.length);
          if (!request.text?.trim() && !request.filePaths?.length && !request.clipboardFiles?.length) throw new Error("消息不能为空。");
          let targetId: string;
          if (request.conversationId) {
            const paths = projectState.projects.map((project) => project.path);
            const groups = await Promise.all([listConversations(), ...paths.map((path) => listConversations(path))]);
            const target = groups.flat().find((item) => item.id === request.conversationId && !item.archived);
            if (!target) throw new Error("会话已删除、归档或项目入口已移除，请重新选择。");
            requireAssistantModel(models, request.text, target.model_config_id || models.activeModelId);
            targetId = target.id;
            await Promise.all([state.refreshConversations(targetId), projectState.refreshProjectConversationMap()]);
          } else {
            requireAssistantModel(models, request.text, models.activeModelId);
            targetId = (await state.createConversationForCurrentScope(null)).id;
          }
          current.current.chat.setActiveConversationId(targetId);
          current.current.openChat();
          setPending({ ...request, targetId });
        } catch (error) {
          processing.current = false;
          await emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id, error: String(error).replace(/^Error: /, "") });
        }
      })().catch((error) => { processing.current = false; current.current.notice(String(error)); });
    }).then((unlisten) => { if (disposed) unlisten(); else stop = unlisten; }).catch((error) => current.current.notice(String(error)));
    return () => { disposed = true; stop?.(); stopVisibility(); };
  }, []);

  useEffect(() => {
    if (pending?.attachmentsReady && chat.activeConversationId !== pending.targetId) {
      setPending(null); processing.current = false;
      void emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: pending.id, error: "会话已切换，请检查附件后重新发送。" }).catch((error) => notice(String(error)));
      return;
    }
    if (!pending || chat.activeConversationId !== pending.targetId || !chat.activeConversation) return;
    const request = pending;
    setPending(null);
    void (async () => {
      try {
        validateAssistantHandoff(chat.busy || chat.uploadingAttachment || chat.speech.isBusy(), request.attachmentsReady ? "" : chat.chatInput, request.attachmentsReady ? 0 : chat.pendingImageAttachments.length);
        const modelId = chat.resolveConversationModelId(request.targetId);
        requireAssistantModel(model, request.text, modelId);
        if ((request.filePaths?.length || request.clipboardFiles?.length) && !request.attachmentsReady) {
          if (request.clipboardFiles?.length) await chat.handleAssistantAttachments(request.filePaths || [], request.clipboardFiles.map(restoreAssistantClipboardFile));
          else await chat.handleAssistantAttachments(request.filePaths!);
          setPending({ ...request, attachmentsReady: true });
          return;
        }
        const text = [request.text.trim(), request.attachmentsReady ? chat.chatInput.trim() : ""].filter(Boolean).join("\n\n") || "请分析上传的文件。";
        await chat.handleAssistantMessage(text);
        await emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id });
      } catch (error) {
        processing.current = false;
        notice(String(error));
        await emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id, error: String(error).replace(/^Error: /, "") });
      } finally { if ((!request.filePaths?.length && !request.clipboardFiles?.length) || request.attachmentsReady) processing.current = false; }
    })().catch((error) => notice(String(error)));
  }, [pending, chat.activeConversationId, chat.activeConversation]);
}
