import { useEffect, useRef, useState } from "react";
import { MantineProvider, Button, Text } from "@mantine/core";
import { FileText, ExternalLink, RotateCcw } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emitTo } from "@tauri-apps/api/event";
import { stopBackgroundAgent } from "../api/agents";
import { readDocumentChatFile, openDocumentConversation } from "../api/documentChat";
import { showAppWindow } from "../api/desktop";
import { useChat, type UseChatReturn } from "../hooks/useChat";
import { useModel } from "../hooks/useModel";
import { useProjects } from "../hooks/useProjects";
import { useSkills } from "../hooks/useSkills";
import { useMcp } from "../hooks/useMcp";
import { useAccessMode } from "../hooks/useAccessMode";
import { useAgentRuntime } from "../hooks/useAgentRuntime";
import { useThemeMode } from "../hooks/useThemeMode";
import type { DocumentSource } from "../lib/documentChat";
import { nanoTheme } from "../theme";
import ChatPane from "./ChatPane";
import ConfirmDialogHost from "./ConfirmDialogHost";
import "./DocumentChat.css";

export default function DocumentChat({ path }: { path: string }) {
  const [notice, setNotice] = useState("");
  const [source, setSource] = useState<DocumentSource | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const chatRef = useRef<UseChatReturn | null>(null);
  const projects = useProjects(setNotice, () => chatRef.current?.conversations || []);
  const model = useModel(setNotice, () => chatRef.current?.activeConversationId || "", (update) => chatRef.current?.setConversations(update), projects.setProjectConversations);
  const skills = useSkills(setNotice);
  const mcp = useMcp(setNotice);
  const { accessMode, setAccessMode } = useAccessMode();
  const { resolvedTheme } = useThemeMode();
  const chat = useChat({ documentSource: source, setNotice, onMemoryCreated: () => {}, projects, model, skills, mcp, showModelConfig: false, activeSettingsTab: "", accessMode });
  chatRef.current = chat;
  const obs = useAgentRuntime(setNotice, chat.activeConversationId);
  const preparedFile = chat.ragFiles.find(file => file.name === source?.name);
  const preparationStatus = chat.indexingRagFileName
    ? `正在准备文档：${chat.indexingRagFileName}`
    : preparedFile
      ? preparedFile.status === "fulltext" ? "全文上下文已就绪" : `索引已就绪 · ${preparedFile.chunk_count} 块`
      : "首次发送时准备文档并保存会话";

  useEffect(() => {
    document.documentElement.classList.add("document-chat-document");
    document.body.classList.add("document-chat-body");
    let active = true;
    setError("");
    void readDocumentChatFile(path).then((document) => { if (active) setSource(document); })
      .catch((error) => { if (active) setError(String(error).replace(/^Error: /, "")); });
    return () => { active = false; document.documentElement.classList.remove("document-chat-document"); document.body.classList.remove("document-chat-body"); };
  }, [path, attempt]);

  useEffect(() => {
    if (chat.activeConversationId) void emitTo("main", "document-chat:changed", chat.activeConversationId).catch(console.error);
  }, [chat.activeConversationId, chat.messages.length]);

  async function openMain() {
    try {
      if (chat.activeConversationId) await openDocumentConversation(chat.activeConversationId);
      else await showAppWindow();
      await getCurrentWindow().minimize();
    } catch (error) { setNotice(String(error)); }
  }

  return <MantineProvider theme={nanoTheme} forceColorScheme={resolvedTheme}>
    <main className="document-chat">
      <header className="document-chat-source">
        <FileText size={22} aria-hidden="true" />
        <div title={path}><strong>{source?.name || path.split(/[\\/]/).pop()}</strong><small>{source ? `已读取 ${source.content.length.toLocaleString()} 字符 · ${preparationStatus}` : error ? "读取失败" : "正在读取文档…"}</small></div>
        <Button variant="subtle" size="compact-sm" leftSection={<ExternalLink size={14} />} onClick={() => void openMain()}>主窗口继续</Button>
      </header>
      {error ? <section className="document-chat-error" role="alert"><h3>无法读取文档</h3><p>{error}</p><Button leftSection={<RotateCcw size={14} />} onClick={() => setAttempt((n) => n + 1)}>重新读取</Button></section> : !source ? <div className="document-chat-loading" role="status">正在提取整份文档中的文字…</div> : <ChatPane {...chat}
        inputPlaceholder="针对这份文档提问…"
        welcome={<section className="document-chat-welcome"><p>想了解这份文档的什么？</p><div>{[{label: "总结全文", question: "总结这份文档的核心内容"}, {label: "关键结论", question: "提取文档中的关键结论"}, {label: "待办事项", question: "这份文档有哪些待办事项？"}].map(({label, question}) => <button key={label} onClick={() => chat.setChatInput(question)}>{label}</button>)}</div></section>}
        project={null} obs={obs} model={model} accessMode={accessMode} onAccessModeChange={setAccessMode}
        handleAttachmentFiles={chat.handleRagFiles}
        handleNewConversation={async () => { setNotice("再次右键文档可开启新的独立对话。"); }}
        handleCloseConversation={() => { void getCurrentWindow().close().catch((e) => setNotice(String(e))); }}
        handleInterruptGeneration={async () => {
          if (!chat.backgroundRunId) return;
          try { await stopBackgroundAgent(chat.backgroundRunId); } catch (error) { setNotice(String(error)); }
        }}
        onOpenModelSettings={() => { void openMain(); setNotice("请在主窗口的系统设置中配置模型，配置后重新打开文档对话。"); }} />}
      {chat.documentPreparationError && <div className="document-chat-notice" role="alert"><Text size="sm">{chat.documentPreparationError}</Text><Button size="compact-xs" disabled={chat.busy} onClick={() => void chat.handleSendMessage()}>重试文档准备</Button></div>}
      {notice && <div className="document-chat-notice" role="status"><Text size="sm">{notice}</Text><button aria-label="关闭提示" onClick={() => setNotice("")}>×</button></div>}
    </main>
    <ConfirmDialogHost />
  </MantineProvider>;
}
