import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, Maximize2, Minimize2, Pencil, PlugZap, Plus, Save, Server, Terminal, Trash2, X } from "lucide-react";
import {
  ActionIcon,
  Button,
  Group,
  Modal,
  NumberInput,
  PasswordInput,
  Select,
  Stack,
  Text,
  TextInput,
  ThemeIcon,
  Tooltip
} from "@mantine/core";
import {
  deleteOpsServer,
  listOpsServers,
  resizeOpsSshSession,
  saveOpsServer,
  sendOpsSshInput,
  startOpsSshSession,
  stopOpsSshSession,
  testOpsSshConnection
} from "../api";
import type { OpsServer, OpsServerDraft, OpsSshEvent } from "../types";
import { confirmAction } from "../lib/dialogs";

interface OpsPanelProps {
  setNotice: (message: string) => void;
}

const emptyDraft: OpsServerDraft = {
  name: "",
  host: "",
  port: 22,
  username: "",
  auth_method: "key",
  key_path: "",
  password: "",
  remote_dir: ""
};

type TerminalSize = {
  cols: number;
  rows: number;
};

const DEFAULT_TERMINAL_SIZE: TerminalSize = { cols: 120, rows: 32 };
const TERMINAL_MIN_COLS = 20;
const TERMINAL_MIN_ROWS = 6;
const TERMINAL_MAX_COLS = 500;
const TERMINAL_MAX_ROWS = 200;

function clampTerminalSize(size: TerminalSize): TerminalSize {
  return {
    cols: Math.min(TERMINAL_MAX_COLS, Math.max(TERMINAL_MIN_COLS, size.cols)),
    rows: Math.min(TERMINAL_MAX_ROWS, Math.max(TERMINAL_MIN_ROWS, size.rows))
  };
}

function measureTerminalSize(terminal: HTMLElement | null): TerminalSize {
  if (!terminal) return DEFAULT_TERMINAL_SIZE;

  const style = window.getComputedStyle(terminal);
  const paddingX = parseFloat(style.paddingLeft || "0") + parseFloat(style.paddingRight || "0");
  const paddingY = parseFloat(style.paddingTop || "0") + parseFloat(style.paddingBottom || "0");
  const contentWidth = Math.max(0, terminal.clientWidth - paddingX);
  const contentHeight = Math.max(0, terminal.clientHeight - paddingY);
  const fontSize = parseFloat(style.fontSize || "13") || 13;
  const lineHeight = parseFloat(style.lineHeight || "") || fontSize * 1.5;

  const probe = document.createElement("span");
  probe.textContent = "mmmmmmmmmm";
  probe.style.position = "absolute";
  probe.style.visibility = "hidden";
  probe.style.whiteSpace = "pre";
  probe.style.fontFamily = style.fontFamily;
  probe.style.fontSize = style.fontSize;
  probe.style.fontWeight = style.fontWeight;
  probe.style.letterSpacing = style.letterSpacing;
  terminal.appendChild(probe);
  const charWidth = Math.max(1, probe.getBoundingClientRect().width / 10);
  terminal.removeChild(probe);

  return clampTerminalSize({
    cols: Math.floor(contentWidth / charWidth),
    rows: Math.floor(contentHeight / lineHeight)
  });
}

function applyTerminalBackspaces(value: string) {
  const output: string[] = [];
  for (const char of value) {
    if (char === "\b" || char === "\u007f") {
      output.pop();
    } else {
      output.push(char);
    }
  }
  return output.join("");
}

function normalizeTerminalOutput(value: string) {
  return applyTerminalBackspaces(value)
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b[P\]^_][\s\S]*?\x1b\\/g, "")
    .replace(/\x1b\[(?![0-9;]*m)[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\x1b(?!\[)[@-_]/g, "")
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n")
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1a\x1c-\x1f]/g, "");
}

function terminalAnsiClasses(codes: number[], currentClasses: string[]) {
  const nextClasses = new Set(currentClasses);
  const normalizedCodes = codes.length > 0 ? codes : [0];

  for (let index = 0; index < normalizedCodes.length; index += 1) {
    const code = normalizedCodes[index];
    if (code === 0) {
      nextClasses.clear();
    } else if (code === 1) {
      nextClasses.add("ansi-bold");
    } else if (code === 2) {
      nextClasses.add("ansi-dim");
    } else if (code === 3) {
      nextClasses.add("ansi-italic");
    } else if (code === 4) {
      nextClasses.add("ansi-underline");
    } else if (code === 22) {
      nextClasses.delete("ansi-bold");
      nextClasses.delete("ansi-dim");
    } else if (code === 23) {
      nextClasses.delete("ansi-italic");
    } else if (code === 24) {
      nextClasses.delete("ansi-underline");
    } else if (code === 39) {
      Array.from(nextClasses).forEach((className) => {
        if (className.startsWith("ansi-fg-")) nextClasses.delete(className);
      });
    } else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) {
      Array.from(nextClasses).forEach((className) => {
        if (className.startsWith("ansi-fg-")) nextClasses.delete(className);
      });
      nextClasses.add(`ansi-fg-${code}`);
    } else if (code === 38 && normalizedCodes[index + 1] === 5 && typeof normalizedCodes[index + 2] === "number") {
      Array.from(nextClasses).forEach((className) => {
        if (className.startsWith("ansi-fg-")) nextClasses.delete(className);
      });
      nextClasses.add(`ansi-fg-256-${normalizedCodes[index + 2]}`);
      index += 2;
    }
  }

  return Array.from(nextClasses);
}

function renderTerminalOutput(value: string): ReactNode[] {
  const output = normalizeTerminalOutput(value);
  const segments: ReactNode[] = [];
  let currentClasses: string[] = [];
  let cursor = 0;
  const ansiPattern = /\x1b\[([0-9;]*)m/g;
  let match: RegExpExecArray | null;

  while ((match = ansiPattern.exec(output)) !== null) {
    if (match.index > cursor) {
      const text = output.slice(cursor, match.index);
      segments.push(
        currentClasses.length > 0
          ? <span className={currentClasses.join(" ")} key={segments.length}>{text}</span>
          : text
      );
    }

    currentClasses = terminalAnsiClasses(
      match[1].split(";").filter(Boolean).map((part) => Number(part)),
      currentClasses
    );
    cursor = match.index + match[0].length;
  }

  if (cursor < output.length) {
    const text = output.slice(cursor);
    segments.push(
      currentClasses.length > 0
        ? <span className={currentClasses.join(" ")} key={segments.length}>{text}</span>
        : text
    );
  }

  return segments;
}

function serverToDraft(server: OpsServer): OpsServerDraft {
  return {
    id: server.id,
    name: server.name,
    host: server.host,
    port: server.port,
    username: server.username,
    auth_method: server.auth_method,
    key_path: server.key_path,
    password: server.password,
    remote_dir: server.remote_dir
  };
}

export default function OpsPanel({ setNotice }: OpsPanelProps) {
  const [servers, setServers] = useState<OpsServer[]>([]);
  const [selectedServerId, setSelectedServerId] = useState("");
  const [draft, setDraft] = useState<OpsServerDraft>(emptyDraft);
  const [sshOutput, setSshOutput] = useState("");
  const [sshSessionId, setSshSessionId] = useState("");
  const [showConfigDialog, setShowConfigDialog] = useState(false);
  const [busyAction, setBusyAction] = useState("");
  const [terminalFullscreen, setTerminalFullscreen] = useState(false);
  const sshSessionIdRef = useRef("");
  const terminalRef = useRef<HTMLPreElement | null>(null);
  const terminalSizeRef = useRef<TerminalSize>(DEFAULT_TERMINAL_SIZE);

  const selectedServer = useMemo(
    () => servers.find((server) => server.id === selectedServerId) || null,
    [servers, selectedServerId]
  );
  const renderedSshOutput = useMemo(() => renderTerminalOutput(sshOutput), [sshOutput]);

  const syncTerminalSize = useCallback((force = false) => {
    const nextSize = measureTerminalSize(terminalRef.current);
    const previousSize = terminalSizeRef.current;
    const changed = nextSize.cols !== previousSize.cols || nextSize.rows !== previousSize.rows;
    terminalSizeRef.current = nextSize;

    if (!sshSessionIdRef.current || (!force && !changed)) {
      return;
    }

    void resizeOpsSshSession(sshSessionIdRef.current, nextSize.cols, nextSize.rows).catch((error) => {
      setNotice(`SSH PTY resize failed: ${String(error)}`);
    });
  }, [setNotice]);

  useEffect(() => {
    void refreshServers();
  }, []);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) {
      return;
    }

    syncTerminalSize(true);
    const resizeObserver = new ResizeObserver(() => syncTerminalSize());
    const handleWindowResize = () => syncTerminalSize();
    resizeObserver.observe(terminal);
    window.addEventListener("resize", handleWindowResize);
    const handleViewportResize = () => syncTerminalSize();
    window.visualViewport?.addEventListener("resize", handleViewportResize);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", handleWindowResize);
      window.visualViewport?.removeEventListener("resize", handleViewportResize);
    };
  }, [syncTerminalSize]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void listen<OpsSshEvent>("ops-ssh", (event) => {
      const payload = event.payload;
      if (payload.session_id !== sshSessionIdRef.current) {
        return;
      }
      if (payload.kind === "data" || payload.kind === "ready") {
        setSshOutput((current) => current + payload.data);
      } else if (payload.kind === "error") {
        setSshOutput((current) => current + `\r\n[error] ${payload.data}\r\n`);
        setNotice(`SSH 会话错误：${payload.data}`);
      } else if (payload.kind === "closed") {
        setSshSessionId("");
        sshSessionIdRef.current = "";
        setSshOutput((current) => current + "\r\n[session closed]\r\n");
      }
      window.setTimeout(() => {
        terminalRef.current?.scrollTo({ top: terminalRef.current.scrollHeight });
      }, 0);
    }).then((dispose) => {
      unlisten = dispose;
    });
    return () => {
      unlisten?.();
      if (sshSessionIdRef.current) {
        void stopOpsSshSession(sshSessionIdRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!selectedServer) {
      setDraft(emptyDraft);
      return;
    }
    setDraft(serverToDraft(selectedServer));
  }, [selectedServer?.id]);

  useEffect(() => {
    if (sshSessionIdRef.current) {
      void stopOpsSshSession(sshSessionIdRef.current);
      sshSessionIdRef.current = "";
      setSshSessionId("");
    }
    setSshOutput("");
  }, [selectedServerId]);

  async function refreshServers(nextSelectedId?: string) {
    try {
      const nextServers = await listOpsServers();
      setServers(nextServers);
      setSelectedServerId((current) => {
        if (nextSelectedId && nextServers.some((server) => server.id === nextSelectedId)) {
          return nextSelectedId;
        }
        if (current && nextServers.some((server) => server.id === current)) {
          return current;
        }
        return nextServers[0]?.id || "";
      });
    } catch (error) {
      setNotice(`加载服务器列表失败：${String(error)}`);
    }
  }

  function updateDraft<K extends keyof OpsServerDraft>(key: K, value: OpsServerDraft[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  function handleNewServer() {
    setDraft(emptyDraft);
    setShowConfigDialog(true);
  }

  async function persistDraft() {
    const saved = await saveOpsServer(draft);
    await refreshServers(saved.id);
    return saved;
  }

  async function handleSaveServer() {
    setBusyAction("save");
    try {
      await persistDraft();
      setNotice("服务器已保存。");
      setShowConfigDialog(false);
    } catch (error) {
      setNotice(`保存服务器失败：${String(error)}`);
    } finally {
      setBusyAction("");
    }
  }

  async function handleDeleteServer() {
    if (!selectedServer) return;
    if (!(await confirmAction(`确定删除服务器「${selectedServer.name}」吗？`))) return;
    setBusyAction("delete");
    try {
      await deleteOpsServer(selectedServer.id);
      setNotice("服务器已删除。");
      await refreshServers();
      setDraft(emptyDraft);
      setSshOutput("");
      setShowConfigDialog(false);
    } catch (error) {
      setNotice(`删除服务器失败：${String(error)}`);
    } finally {
      setBusyAction("");
    }
  }

  async function handleTestConnection() {
    setBusyAction("ssh");
    try {
      const saved = await persistDraft();
      const output = await testOpsSshConnection(saved.id);
      setSshOutput(output);
      setNotice("SSH 连接成功。");
    } catch (error) {
      const message = String(error);
      setSshOutput(message);
      setNotice(`SSH 连接失败：${message}`);
    } finally {
      setBusyAction("");
    }
  }

  async function handleStartSshSession() {
    if (!selectedServer) {
      setNotice("请先保存并选择一台服务器。");
      return;
    }
    setBusyAction("session");
    try {
      if (sshSessionIdRef.current) {
        await stopOpsSshSession(sshSessionIdRef.current);
      }
      setSshOutput("");
      const terminalSize = measureTerminalSize(terminalRef.current);
      terminalSizeRef.current = terminalSize;
      const sessionId = await startOpsSshSession(selectedServer.id, terminalSize);
      sshSessionIdRef.current = sessionId;
      setSshSessionId(sessionId);
      window.setTimeout(() => {
        terminalRef.current?.focus();
        syncTerminalSize(true);
      }, 0);
    } catch (error) {
      const message = String(error);
      setSshOutput(message);
      setNotice(`SSH 会话启动失败：${message}`);
    } finally {
      setBusyAction("");
    }
  }

  async function handleStopSshSession() {
    if (!sshSessionIdRef.current) {
      return;
    }
    const sessionId = sshSessionIdRef.current;
    sshSessionIdRef.current = "";
    setSshSessionId("");
    await stopOpsSshSession(sessionId);
  }

  function getTerminalSelection() {
    const selection = window.getSelection();
    const terminal = terminalRef.current;
    if (!selection || !terminal || selection.rangeCount === 0) {
      return "";
    }

    const range = selection.getRangeAt(0);
    if (!terminal.contains(range.commonAncestorContainer)) {
      return "";
    }

    return selection.toString();
  }

  async function copyTerminalSelection() {
    const selectedText = getTerminalSelection();
    if (!selectedText) {
      return false;
    }

    try {
      await navigator.clipboard.writeText(selectedText);
      return true;
    } catch (error) {
      setNotice(`复制失败：${String(error)}`);
      return false;
    }
  }

  async function pasteIntoTerminal(text?: string) {
    if (!sshSessionIdRef.current) {
      setNotice("请先连接 SSH 会话。");
      return;
    }

    try {
      const clipboardText = text ?? await navigator.clipboard.readText();
      if (!clipboardText) {
        return;
      }
      await sendOpsSshInput(sshSessionIdRef.current, clipboardText);
      window.setTimeout(() => terminalRef.current?.focus(), 0);
    } catch (error) {
      setNotice(`粘贴失败：${String(error)}`);
    }
  }

  function mapTerminalKey(event: React.KeyboardEvent<HTMLElement>) {
    if (event.ctrlKey && event.key.toLowerCase() === "c") return "\u0003";
    if (event.ctrlKey && event.key.toLowerCase() === "d") return "\u0004";
    if (event.ctrlKey && event.key.toLowerCase() === "l") return "\u000c";
    if (event.key === "Enter") return "\r";
    if (event.key === "Backspace") return "\u007f";
    if (event.key === "Tab") return "\t";
    if (event.key === "ArrowUp") return "\u001b[A";
    if (event.key === "ArrowDown") return "\u001b[B";
    if (event.key === "ArrowRight") return "\u001b[C";
    if (event.key === "ArrowLeft") return "\u001b[D";
    if (event.key === "Escape") return "\u001b";
    if (!event.ctrlKey && !event.metaKey && event.key.length === 1) return event.key;
    return "";
  }

  function handleTerminalKeyDown(event: React.KeyboardEvent<HTMLElement>) {
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c") {
      event.preventDefault();
      void copyTerminalSelection();
      return;
    }
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "v") {
      event.preventDefault();
      void pasteIntoTerminal();
      return;
    }
    if (!sshSessionIdRef.current) {
      return;
    }
    const input = mapTerminalKey(event);
    if (!input) {
      return;
    }
    event.preventDefault();
    void sendOpsSshInput(sshSessionIdRef.current, input);
  }

  function handleTerminalPaste(event: React.ClipboardEvent<HTMLElement>) {
    event.preventDefault();
    void pasteIntoTerminal(event.clipboardData.getData("text"));
  }

  function handleTerminalContextMenu(event: React.MouseEvent<HTMLElement>) {
    event.preventDefault();
    if (getTerminalSelection()) {
      void copyTerminalSelection();
      return;
    }
    void pasteIntoTerminal();
  }

  function toggleTerminalFullscreen() {
    setTerminalFullscreen((current) => !current);
    window.setTimeout(() => {
      terminalRef.current?.focus();
      syncTerminalSize(true);
    }, 0);
  }

  return (
    <section className={terminalFullscreen ? "ops-panel terminal-fullscreen" : "ops-panel"}>
      <header className="ops-header">
        <div>
          <Server size={20} />
          <div className="ops-header-title">
            <strong>运维区</strong>
            <span>服务器管理 · SSH 连接 · 远程命令</span>
          </div>
        </div>
      </header>

      <div className={terminalFullscreen ? "ops-layout terminal-fullscreen" : "ops-layout"}>
        <aside className="ops-server-list">
          <div className="ops-section-title">
            <div>
              <span>服务器列表</span>
              <small>{servers.length} 台</small>
            </div>
            <Tooltip label="新增服务器">
              <ActionIcon variant="light" color="nanoBlue" onClick={handleNewServer} aria-label="新增服务器">
                <Plus size={15} />
              </ActionIcon>
            </Tooltip>
          </div>
          <div className="ops-server-items">
            {servers.map((server) => (
              <div
                key={server.id}
                className={server.id === selectedServerId ? "ops-server-item active" : "ops-server-item"}
                role="button"
                tabIndex={0}
                onClick={() => setSelectedServerId(server.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    setSelectedServerId(server.id);
                  }
                }}
              >
                <div className="ops-server-item-info">
                  <strong>{server.name}</strong>
                  <span>{server.username}@{server.host}:{server.port}</span>
                </div>
                <ActionIcon
                  className="ops-server-edit-btn"
                  variant="subtle"
                  color="gray"
                  aria-label={`编辑 ${server.name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedServerId(server.id);
                    setDraft(serverToDraft(server));
                    setShowConfigDialog(true);
                  }}
                >
                  <Pencil size={15} />
                </ActionIcon>
              </div>
            ))}
            {servers.length === 0 && (
              <div className="empty ops-empty">还没有服务器</div>
            )}
          </div>
        </aside>

        <div className="ops-workspace">
          <section className="ops-card">
            <div className="ops-card-header">
              <div>
                <Terminal size={17} />
                <strong>SSH 交互</strong>
              </div>
              <div className="ops-actions">
                <Tooltip label={terminalFullscreen ? "退出全屏" : "全屏"}>
                  <ActionIcon
                  variant="subtle"
                  color="gray"
                  onClick={toggleTerminalFullscreen}
                  aria-label={terminalFullscreen ? "退出全屏" : "全屏"}
                >
                  {terminalFullscreen ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
                  </ActionIcon>
                </Tooltip>
                {sshSessionId ? (
                  <Button color="red" variant="light" leftSection={<X size={16} />} onClick={() => void handleStopSshSession()}>
                    断开
                  </Button>
                ) : (
                  <Button leftSection={<PlugZap size={16} />} onClick={() => void handleStartSshSession()} loading={busyAction === "session"} disabled={!selectedServer}>
                    连接
                  </Button>
                )}
              </div>
            </div>

            <pre
              ref={terminalRef}
              className="ops-output ops-terminal-output"
              tabIndex={0}
              onKeyDown={handleTerminalKeyDown}
              onPaste={handleTerminalPaste}
              onContextMenu={handleTerminalContextMenu}
              onClick={() => terminalRef.current?.focus()}
            >
              {sshOutput ? renderedSshOutput : "选择服务器后点击连接，在这里直接输入 SSH 交互命令。"}
            </pre>
          </section>
        </div>
        {/*
          AI 运维协作 UI 暂时隐藏，先调试服务器管理、SSH 连接和远程命令基础功能。
          后端 ask_ops_ai 命令与 API 入口保留，后续恢复 UI 时可重新接入。
        */}
      </div>
      {showConfigDialog && (
        <Modal
          opened
          onClose={() => setShowConfigDialog(false)}
          size="lg"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color={draft.id ? "nanoBlue" : "teal"} size="md">
                <Server size={18} />
              </ThemeIcon>
              <Text fw={650}>{draft.id ? "编辑服务器" : "新增服务器"}</Text>
            </Group>
          }
        >
          <Stack gap="lg">

            <div className="ops-form-grid">
              <TextInput label="名称" value={draft.name} onChange={(event) => updateDraft("name", event.currentTarget.value)} placeholder="生产服务器" />
              <TextInput label="主机" value={draft.host} onChange={(event) => updateDraft("host", event.currentTarget.value)} placeholder="192.168.1.10 / example.com" />
              <NumberInput label="端口" min={1} max={65535} value={draft.port || 22} onChange={(value) => updateDraft("port", Number(value) || 22)} />
              <TextInput label="用户名" value={draft.username} onChange={(event) => updateDraft("username", event.currentTarget.value)} placeholder="root / ubuntu" />
              <Select
                label="认证方式"
                value={draft.auth_method}
                data={[
                  { value: "key", label: "密钥路径" },
                  { value: "agent", label: "SSH Agent / 本机配置" },
                  { value: "password", label: "用户名密码" }
                ]}
                onChange={(value) => value && updateDraft("auth_method", value)}
                allowDeselect={false}
              />
              {draft.auth_method === "password" ? (
                <PasswordInput label="密码" value={draft.password} onChange={(event) => updateDraft("password", event.currentTarget.value)} placeholder="服务器登录密码" />
              ) : (
                <TextInput label="私钥路径" value={draft.key_path} onChange={(event) => updateDraft("key_path", event.currentTarget.value)} placeholder="C:\Users\...\id_rsa" />
              )}
              <TextInput className="ops-form-wide" label="默认远程目录" value={draft.remote_dir} onChange={(event) => updateDraft("remote_dir", event.currentTarget.value)} placeholder="/opt/app/" />
            </div>

            <Group justify="flex-end">
              {draft.id && selectedServer && (
                <Button
                  color="red"
                  variant="light"
                  leftSection={<Trash2 size={16} />}
                  onClick={handleDeleteServer}
                  loading={busyAction === "delete"}
                  mr="auto"
                >
                  删除服务器
                </Button>
              )}
              <Button
                variant="default"
                leftSection={<CheckCircle2 size={16} />}
                onClick={handleTestConnection}
                loading={busyAction === "ssh"}
                disabled={busyAction === "save"}
              >
                测试连接
              </Button>
              <Button variant="default" onClick={() => setShowConfigDialog(false)}>取消</Button>
              <Button
                color="nanoBlue"
                leftSection={<Save size={16} />}
                onClick={handleSaveServer}
                loading={busyAction === "save"}
              >
                {draft.id ? "保存修改" : "创建服务器"}
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}
    </section>
  );
}
