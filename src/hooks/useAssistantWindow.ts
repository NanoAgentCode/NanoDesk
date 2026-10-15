import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { resizeAssistant } from "../api/assistant";
import { createAssistantBlurTimer } from "../lib/assistantFocus";
import { registerAssistantContextMenu } from "../lib/assistantContextMenu";
import { createAssistantTransition, type AssistantPhase } from "../lib/assistantTransition";

export function useAssistantWindow(choosingFiles: boolean, hasMention: boolean, setError: (error: string) => void) {
  const [expanded, setExpanded] = useState(false);
  const [phase, setPhase] = useState<AssistantPhase>("icon");
  const transition = useRef<ReturnType<typeof createAssistantTransition> | null>(null);

  useEffect(() => {
    if (!expanded || choosingFiles) return;
    let disposed = false;
    let revision = 0;
    const timer = createAssistantBlurTimer(() => setExpanded(false));
    const nativeWindow = getCurrentWindow();
    const subscription = nativeWindow.onFocusChanged(({ payload }) => {
      if (disposed) return;
      revision++; timer.setFocused(payload);
    });
    void subscription.then(async () => {
      const initialRevision = revision;
      const focused = await nativeWindow.isFocused();
      if (!disposed && revision === initialRevision) timer.setFocused(focused);
    }).catch((err) => { if (!disposed) setError(String(err)); });
    return () => {
      disposed = true; timer.dispose();
      void subscription.then((stop) => stop()).catch(console.error);
    };
  }, [expanded, choosingFiles]);

  useEffect(() => registerAssistantContextMenu(document, (err) => setError(String(err))), []);

  useEffect(() => {
    document.body.classList.add("assistant-surface");
    document.getElementById("nano-startup")?.remove();
    return () => document.body.classList.remove("assistant-surface");
  }, []);
  useEffect(() => {
    const controller = createAssistantTransition(resizeAssistant, setPhase, (err, expanding) => {
      setError(String(err));
      setExpanded(!expanding);
    });
    transition.current = controller;
    return () => { controller.dispose(); transition.current = null; };
  }, []);
  useEffect(() => {
    transition.current?.update(expanded ? hasMention ? "picker" : "input" : "icon");
  }, [expanded, hasMention]);
  return { expanded, setExpanded, phase };
}
