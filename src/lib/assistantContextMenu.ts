import { Menu } from "@tauri-apps/api/menu";
import { setAssistantVisible } from "../api/assistant";

export function registerAssistantContextMenu(surface: EventTarget, onError: (error: unknown) => void) {
  let menu: Promise<Menu> | undefined;
  let disposed = false;
  function onContextMenu(event: Event) {
    event.preventDefault();
    menu ||= Menu.new({ items: [{
      id: "assistant-close",
      text: "关闭",
      action: () => { void setAssistantVisible(false).catch(onError); }
    }] });
    void menu.then((ready) => { if (!disposed) return ready.popup(); }).catch((error) => {
      menu = undefined;
      onError(error);
    });
  }
  surface.addEventListener("contextmenu", onContextMenu, { capture: true });
  return () => {
    disposed = true;
    surface.removeEventListener("contextmenu", onContextMenu, { capture: true });
    void menu?.then((ready) => ready.close()).catch(onError);
  };
}
