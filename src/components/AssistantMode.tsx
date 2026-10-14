import { useEffect, useRef, useState } from "react";
import { ArrowUp, MessageCircle, Plus, X, ExternalLink } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listConversationProjectPaths, listConversations } from "../api/conversations";
import { showAppWindow } from "../api/desktop";
import { resizeAssistant, submitAssistantRequest } from "../api/assistant";
import { filterAssistantConversations, getConversationMention, removeConversationMention } from "../lib/assistantMode";
import type { Conversation } from "../types";
import "./AssistantMode.css";
import brandIcon from "../../src-tauri/icons/icon.svg";
import { registerAssistantContextMenu } from "../lib/assistantContextMenu";

export default function AssistantMode() {
  const [expanded, setExpanded] = useState(false);
  const [text, setText] = useState("");
  const [cursor, setCursor] = useState(0);
  const [selected, setSelected] = useState<Conversation | null>(null);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const dragged = useRef(false);
  const origin = useRef({ x: 0, y: 0 });
  const mention = getConversationMention(text, cursor);
  const suggestions = filterAssistantConversations(conversations, mention?.query || "");

  useEffect(() => registerAssistantContextMenu(document, (err) => setError(String(err))), []);

  useEffect(() => {
    document.body.classList.add("assistant-surface");
    document.getElementById("nano-startup")?.remove();
    return () => document.body.classList.remove("assistant-surface");
  }, []);
  useEffect(() => {
    void resizeAssistant(expanded ? mention ? "picker" : "input" : "icon").then(() => {
      if (expanded) input.current?.focus();
    }).catch((err) => setError(String(err)));
  }, [expanded, !!mention]);
  useEffect(() => {
    if (!expanded) return;
    let active = true;
    setLoading(true);
    void (async () => {
      const paths = await listConversationProjectPaths();
      const groups = await Promise.all([listConversations(), ...paths.map((path) => listConversations(path))]);
      if (active) setConversations([...new Map(groups.flat().map((item) => [item.id, item])).values()]);
    })().catch((err) => { if (active) setError(`读取会话失败：${String(err)}`); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [expanded]);
  useEffect(() => { setIndex(0); }, [mention?.query]);

  function selectConversation(conversation: Conversation) {
    if (!mention) return;
    const next = removeConversationMention(text, mention);
    setSelected(conversation); setText(next); setCursor(mention.start); setError("");
    requestAnimationFrame(() => { input.current?.focus(); input.current?.setSelectionRange(mention.start, mention.start); });
  }
  async function send() {
    if (busy || !text.trim() || mention) return;
    setBusy(true); setError("");
    try {
      await submitAssistantRequest(text.trim(), selected?.id || null);
      setText(""); setCursor(0); setSelected(null); setExpanded(false);
    } catch (err) { setError(String(err).replace(/^Error: /, "")); }
    finally { setBusy(false); }
  }
  if (!expanded) return <button className="assistant-orb" type="button" aria-label="打开桌面助手" title="点击提问，拖动移动"
    onPointerDown={(event) => { dragged.current = false; origin.current = { x: event.clientX, y: event.clientY }; }}
    onPointerMove={(event) => {
      if (event.buttons === 1 && !dragged.current && Math.hypot(event.clientX - origin.current.x, event.clientY - origin.current.y) > 5) {
        dragged.current = true; void getCurrentWindow().startDragging().catch(() => { dragged.current = false; });
      }
    }}
    onClick={() => { if (!dragged.current) setExpanded(true); }}><img src={brandIcon} alt="NanoDesk" /></button>;

  return <main className="assistant-panel">
    {mention && <section className="assistant-picker" aria-label="选择已有会话">
      <div className="assistant-picker-heading">继续已有会话 <span>↑ ↓ 选择 · Enter 确认</span></div>
      <div role="listbox" id="assistant-conversations">
        {suggestions.map((item, i) => <button type="button" role="option" aria-selected={i === index} id={`assistant-option-${i}`} key={item.id}
          className={i === index ? "active" : ""} onClick={() => selectConversation(item)}>
          <MessageCircle size={17} /><span><strong>{item.title}</strong>{item.project_path && <small>{item.project_path}</small>}</span>
        </button>)}
        {!suggestions.length && <p>{loading ? "正在读取会话…" : "没有匹配的会话"}</p>}
      </div>
    </section>}
    <div className="assistant-toolbar"><span data-tauri-drag-region>桌面助手</span>
      <button type="button" aria-label="打开主窗口" onClick={() => void showAppWindow().catch((err) => setError(String(err)))}><ExternalLink size={14} /></button>
      <button type="button" aria-label="收起助手" disabled={busy} onClick={() => setExpanded(false)}><X size={16} /></button>
    </div>
    <div className="assistant-input-bar">
      <button className="assistant-new" type="button" aria-label="开始新聊天" disabled={busy} onClick={() => { setSelected(null); setError(""); input.current?.focus(); }}><Plus size={24} /></button>
      {selected && <button className="assistant-target" type="button" title={`继续：${selected.title}；点击取消`} disabled={busy} onClick={() => setSelected(null)}><MessageCircle size={16} /><span>{selected.title}</span><X size={12} /></button>}
      <input ref={input} aria-label="助手消息" placeholder={selected ? "继续这个会话…" : "开始新聊天，或 @ 选择已有会话"} value={text} disabled={busy}
        role="combobox" aria-expanded={!!mention} aria-controls="assistant-conversations" aria-autocomplete="list" aria-activedescendant={mention && suggestions[index] ? `assistant-option-${index}` : undefined}
        onChange={(event) => { setText(event.target.value); setCursor(event.target.selectionStart || 0); setError(""); }}
        onSelect={(event) => setCursor(event.currentTarget.selectionStart || 0)}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing || event.keyCode === 229) return;
          if (event.key === "Escape") { setExpanded(false); return; }
          if (mention && ["ArrowUp", "ArrowDown"].includes(event.key)) {
            event.preventDefault(); setIndex((current) => suggestions.length ? (current + (event.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length : 0);
          }
          if (event.key === "Enter") { event.preventDefault(); if (mention) { if (suggestions[index]) selectConversation(suggestions[index]); } else void send(); }
        }} />
      <button className="assistant-send" type="button" aria-label="发送消息" disabled={busy || !text.trim() || !!mention} onClick={() => void send()}><ArrowUp size={24} /></button>
    </div>
    {error && <p className="assistant-error" role="alert">{error}</p>}
  </main>;
}
