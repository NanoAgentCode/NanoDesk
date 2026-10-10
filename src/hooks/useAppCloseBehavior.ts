import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { minimizeToTray, quitApp, showAppWindow } from "../api/desktop";
import {
  getStoredCloseAction,
  getStoredClosePreferences,
  getStoredCloseSkipPrompt,
  setStoredClosePreferences,
  subscribeClosePreferencesChanged,
  type CloseAction
} from "../lib/closeBehavior";
import { APP_STORAGE_PREFIX } from "../config/brand";

export function useAppCloseBehavior(setNotice: (message: string) => void) {
  const [closePromptOpen, setClosePromptOpen] = useState(false);
  const [closeAction, setCloseAction] = useState<CloseAction>(() => {
    return getStoredCloseAction();
  });
  const [closeDontAsk, setCloseDontAsk] = useState(() => {
    return getStoredCloseSkipPrompt();
  });
  const closePromptOpenRef = useRef(false);
  const performCloseAction = useCallback(async (action: CloseAction) => {
    try {
      if (action === "tray") {
        await minimizeToTray();
        return;
      }
      await quitApp();
    } catch (error) {
      setNotice(String(error));
    }
  }, []);

  useEffect(() => {
    closePromptOpenRef.current = closePromptOpen;
  }, [closePromptOpen]);

  useEffect(() => {
    return subscribeClosePreferencesChanged((preferences) => {
      setCloseAction(preferences.action);
      setCloseDontAsk(preferences.skipPrompt);
    });
  }, []);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlistenClose: (() => void) | undefined;
    let unlistenTrayShow: (() => void) | undefined;

    void appWindow
      .onCloseRequested((event) => {
        event.preventDefault();
        if (closePromptOpenRef.current) {
          return;
        }

        const savedPreferences = getStoredClosePreferences();
        if (savedPreferences.skipPrompt) {
          void performCloseAction(savedPreferences.action);
          return;
        }

        setCloseAction(savedPreferences.action);
        setCloseDontAsk(savedPreferences.skipPrompt);
        setClosePromptOpen(true);
      })
      .then((unlisten) => {
        unlistenClose = unlisten;
      });

    void appWindow
      .listen(`${APP_STORAGE_PREFIX}-show-window`, () => {
        void showAppWindow();
      })
      .then((unlisten) => {
        unlistenTrayShow = unlisten;
      });

    return () => {
      unlistenClose?.();
      unlistenTrayShow?.();
    };
  }, [performCloseAction]);

  function handleCancelClosePrompt() {
    setClosePromptOpen(false);
  }

  function handleConfirmClosePrompt() {
    setStoredClosePreferences({ action: closeAction, skipPrompt: closeDontAsk });
    setClosePromptOpen(false);
    void performCloseAction(closeAction);
  }

  return {
    closePromptOpen,
    closeAction,
    setCloseAction,
    closeDontAsk,
    setCloseDontAsk,
    handleCancelClosePrompt,
    handleConfirmClosePrompt
  };
}
