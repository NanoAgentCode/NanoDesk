import { useRef } from "react";
import type { CSSProperties, ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import brandIcon from "./assistant-icon.svg";
import { ASSISTANT_TRANSITION_MS, type AssistantPhase } from "../lib/assistantTransition";

interface Props {
  phase: AssistantPhase;
  expanded: boolean;
  running: boolean;
  onActivate: () => void;
  children: ReactNode;
}

export default function AssistantSurface({ phase, expanded, running, onActivate, children }: Props) {
  const dragged = useRef(false);
  const origin = useRef({ x: 0, y: 0 });
  const panelVisible = phase !== "icon" && phase !== "preparing";
  return <>
    <button className={`assistant-orb${running ? " assistant-orb--running" : ""}`} type="button"
      aria-label="打开桌面助手" aria-busy={running} aria-expanded={expanded}
      aria-controls={panelVisible ? "assistant-panel" : undefined}
      title={`${running ? "任务进行中；" : ""}${expanded ? "点击收起，拖动移动" : "点击提问，拖动移动"}`}
      onPointerDown={(event) => { dragged.current = false; origin.current = { x: event.clientX, y: event.clientY }; }}
      onPointerMove={(event) => {
        if (event.buttons === 1 && !dragged.current && Math.hypot(event.clientX - origin.current.x, event.clientY - origin.current.y) > 5) {
          dragged.current = true;
          void getCurrentWindow().startDragging().catch(() => { dragged.current = false; });
        }
      }}
      onClick={() => { if (!dragged.current) onActivate(); }}>
      <img src={brandIcon} alt="NanoDesk" />
    </button>
    {panelVisible && <main id="assistant-panel" className={`assistant-panel assistant-panel--${phase}`}
      style={{ "--assistant-transition-duration": `${ASSISTANT_TRANSITION_MS}ms` } as CSSProperties}
      aria-hidden={!expanded} {...(!expanded ? { inert: "" } : {})}>
      {children}
    </main>}
  </>;
}
