import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { APP_STORAGE_PREFIX } from "../config/brand";
import { ASSISTANT_REPLY_EVENT, ASSISTANT_REQUEST_EVENT, type AssistantReply, type AssistantRequest } from "../lib/assistantMode";

const visibilityKey = `${APP_STORAGE_PREFIX}-assistant-visible`;
export const getStoredAssistantVisible = () => localStorage.getItem(visibilityKey) === "true";
export function subscribeAssistantVisibility(listener: (visible: boolean) => void) {
  let disposed = false;
  const subscription = listen<boolean>("assistant:visibility", ({ payload }) => {
    if (disposed) return;
    localStorage.setItem(visibilityKey, String(payload));
    listener(payload);
  });
  const onStorage = (event: StorageEvent) => {
    if (event.key === visibilityKey || event.key === null) listener(getStoredAssistantVisible());
  };
  window.addEventListener("storage", onStorage);
  return () => {
    disposed = true;
    window.removeEventListener("storage", onStorage);
    void subscription.then((stop) => stop()).catch(console.error);
  };
}
export async function setAssistantVisible(visible: boolean) {
  await invoke("set_assistant_visible", { visible });
  localStorage.setItem(visibilityKey, String(visible));
}
export const restoreAssistantVisibility = () => invoke("set_assistant_visible", { visible: getStoredAssistantVisible() });
export const resizeAssistant = (presentation: "icon" | "input" | "picker") => invoke("resize_assistant", { presentation });

export async function submitAssistantRequest(text: string, conversationId: string | null, filePaths: string[] = []) {
  const id = crypto.randomUUID();
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await new Promise<void>((resolve, reject) => {
      void listen<AssistantReply>(ASSISTANT_REPLY_EVENT, ({ payload }) => {
        if (payload.id !== id) return;
        payload.error ? reject(new Error(payload.error)) : resolve();
      }).then(async (stop) => {
        unlisten = stop;
        timer = setTimeout(() => reject(new Error("未收到确认，请到主窗口检查消息是否已发送后再操作。")), filePaths.length ? 180000 : 30000);
        try {
          await emitTo("main", ASSISTANT_REQUEST_EVENT, { id, text, conversationId, ...(filePaths.length ? { filePaths } : {}) } satisfies AssistantRequest);
        } catch (error) { reject(error); }
      }).catch(reject);
    });
  } finally { unlisten?.(); if (timer) clearTimeout(timer); }
}
