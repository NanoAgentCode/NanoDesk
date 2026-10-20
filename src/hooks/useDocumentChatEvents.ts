import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { listConversations } from "../api/conversations";
import type { UseChatReturn } from "./useChat";

export function useDocumentChatEvents(chat: UseChatReturn, openChat: () => void, notice: (text: string) => void) {
  const current = useRef({ chat, openChat, notice });
  current.current = { chat, openChat, notice };
  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const subscriptions = [
      listen<string>("document-chat:changed", () => { void listConversations().then((conversations) => current.current.chat.setConversations(conversations)).catch((e) => current.current.notice(String(e))); }),
      listen<string>("document-chat:open", ({ payload: id }) => {
        current.current.openChat();
        void current.current.chat.refreshConversations(id).then(() => current.current.chat.loadMessages(id)).catch((e) => current.current.notice(String(e)));
      }),
      listen<string>("document-chat:error", ({ payload }) => current.current.notice(payload))
    ];
    for (const subscription of subscriptions) void subscription.then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch((e) => current.current.notice(String(e)));
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, []);
}
