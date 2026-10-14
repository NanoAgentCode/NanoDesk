import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { listConversations } from "../api/conversations";
import { restoreAssistantVisibility } from "../api/assistant";
import { ASSISTANT_REPLY_EVENT, ASSISTANT_REQUEST_EVENT, validateAssistantHandoff, type AssistantRequest } from "../lib/assistantMode";
import type { UseChatReturn } from "./useChat";
import type { UseProjectsReturn } from "./useProjects";
import type { UseModelReturn } from "./useModel";

function requireAssistantModel(model: UseModelReturn, text: string, savedId: string) {
  const routedId = model.routing.enabled ? model.routing.resolve(text)?.modelId : null;
  const id = routedId || (model.routing.enabled ? savedId : model.routing.fixedModelIds.includes(savedId) ? savedId : model.routing.fixedModelIds[0]);
  if (!id || !model.models.some((item) => item.id === id)) throw new Error("请先在主窗口配置并选择聊天模型。");
}

export function useAssistantHandoff(chat: UseChatReturn, projects: UseProjectsReturn, model: UseModelReturn, openChat: () => void, notice: (text: string) => void) {
  const current = useRef({ chat, projects, model, openChat, notice });
  current.current = { chat, projects, model, openChat, notice };
  const processing = useRef(false);
  const [pending, setPending] = useState<(AssistantRequest & { targetId: string }) | null>(null);

  useEffect(() => {
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
          if (!request.text?.trim()) throw new Error("消息不能为空。");
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
    return () => { disposed = true; stop?.(); };
  }, []);

  useEffect(() => {
    if (!pending || chat.activeConversationId !== pending.targetId || !chat.activeConversation) return;
    const request = pending;
    setPending(null);
    void (async () => {
      try {
        validateAssistantHandoff(chat.busy || chat.uploadingAttachment || chat.speech.isBusy(), chat.chatInput, chat.pendingImageAttachments.length);
        const modelId = chat.resolveConversationModelId(request.targetId);
        requireAssistantModel(model, request.text, modelId);
        await Promise.all([
          chat.handleAssistantMessage(request.text),
          emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id })
        ]);
      } catch (error) {
        notice(String(error));
        await emitTo("assistant", ASSISTANT_REPLY_EVENT, { id: request.id, error: String(error).replace(/^Error: /, "") });
      } finally { processing.current = false; }
    })().catch((error) => notice(String(error)));
  }, [pending, chat.activeConversationId, chat.activeConversation]);
}
