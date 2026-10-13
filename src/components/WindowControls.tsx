import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import "./WindowControls.css";

export default function WindowControls() {
  return <div className="window-controls">
    <div className="window-drag-region" data-tauri-drag-region />
    <button type="button" aria-label="最小化" onClick={() => void getCurrentWindow().minimize()}><Minus size={14} /></button>
    <button type="button" aria-label="最大化或还原" onClick={() => void getCurrentWindow().toggleMaximize()}><Square size={12} /></button>
    <button type="button" className="window-close" aria-label="关闭" onClick={() => void getCurrentWindow().close()}><X size={16} /></button>
  </div>;
}
