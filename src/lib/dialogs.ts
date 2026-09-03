import { confirm as tauriConfirm, message as tauriMessage } from "@tauri-apps/plugin-dialog";
import { APP_NAME } from "../config/brand";

export type DialogKind = "info" | "warning" | "error";
export type ConfirmActionHandler = (content: string, kind: DialogKind) => Promise<boolean>;

const APP_DIALOG_TITLE = APP_NAME;
let customConfirmHandler: ConfirmActionHandler | null = null;

export function registerConfirmActionHandler(handler: ConfirmActionHandler | null) {
  customConfirmHandler = handler;
}

export async function confirmAction(content: string, kind: DialogKind = "warning") {
  if (customConfirmHandler) {
    return customConfirmHandler(content, kind);
  }

  try {
    return await tauriConfirm(content, {
      title: APP_DIALOG_TITLE,
      kind
    });
  } catch (error) {
    console.error("Failed to show confirmation dialog:", error);
    return window.confirm(content);
  }
}

export async function showAppMessage(content: string, kind: DialogKind = "info") {
  try {
    await tauriMessage(content, {
      title: APP_DIALOG_TITLE,
      kind
    });
  } catch (error) {
    console.error("Failed to show message dialog:", error);
  }
}
