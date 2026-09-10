import { useEffect, useMemo, useRef, useState } from "react";
import {
  listMcpServers,
  restoreMcpServers,
  saveMcpServer,
  deleteMcpServer,
  connectMcpServer,
  disconnectMcpServer,
  refreshMcpTools
} from "../api";
import type { McpServerDraft, McpServerView } from "../types";
import { formatStdioCommandLine, parseJsonStringArray, parseStdioCommandLine } from "../lib/stdioCommand";
import { confirmAction } from "../lib/dialogs";

export const emptyMcpDraft: McpServerDraft = {
  name: "filesystem-server",
  transport: "stdio",
  command: "npx",
  args_json: "[\"-y\", \"@modelcontextprotocol/server-filesystem\", \"C:\\\\Users\\\\13439\\\\Desktop\"]",
  env_json: "{}",
  url: "",
  headers_json: "{}",
  working_dir: "",
  enabled: true
};

export interface UseMcpReturn {
  mcpServers: McpServerView[];
  setMcpServers: React.Dispatch<React.SetStateAction<McpServerView[]>>;
  selectedMcpServerId: string;
  setSelectedMcpServerId: React.Dispatch<React.SetStateAction<string>>;
  mcpDraft: McpServerDraft;
  setMcpDraft: React.Dispatch<React.SetStateAction<McpServerDraft>>;
  stdioCommandLine: string;
  setStdioCommandLine: React.Dispatch<React.SetStateAction<string>>;
  mcpBusyId: string;
  setMcpBusyId: React.Dispatch<React.SetStateAction<string>>;
  selectedMcpServer: McpServerView | null;
  refreshMcpServers: (selectId?: string) => Promise<void>;
  updateMcpServerView: (view: McpServerView) => void;
  handleNewMcpServer: () => void;
  handleSaveMcpServer: () => Promise<void>;
  handleDeleteMcpServer: () => Promise<void>;
  handleConnectMcpServer: (id: string) => Promise<void>;
  handleDisconnectMcpServer: (id: string) => Promise<void>;
  handleRefreshMcpTools: (id: string) => Promise<void>;
}

export function useMcp(setNotice: (message: string) => void): UseMcpReturn {
  const [mcpServers, setMcpServers] = useState<McpServerView[]>([]);
  const [mcpDraft, setMcpDraft] = useState<McpServerDraft>(emptyMcpDraft);
  const [stdioCommandLine, setStdioCommandLine] = useState(formatStdioCommandLine(emptyMcpDraft));
  const [selectedMcpServerId, setSelectedMcpServerId] = useState("");
  const [mcpBusyId, setMcpBusyId] = useState("");
  const restoredOnStartupRef = useRef(false);

  const selectedMcpServer = useMemo(
    () => mcpServers.find((server) => server.config.id === selectedMcpServerId) || null,
    [mcpServers, selectedMcpServerId]
  );

  useEffect(() => {
    if (!selectedMcpServer) {
      setMcpDraft(emptyMcpDraft);
      setStdioCommandLine(formatStdioCommandLine(emptyMcpDraft));
      return;
    }

    const nextDraft = {
      id: selectedMcpServer.config.id,
      name: selectedMcpServer.config.name,
      transport: selectedMcpServer.config.transport || "stdio",
      command: selectedMcpServer.config.command,
      args_json: selectedMcpServer.config.args_json,
      env_json: selectedMcpServer.config.env_json,
      url: selectedMcpServer.config.url,
      headers_json: selectedMcpServer.config.headers_json,
      working_dir: selectedMcpServer.config.working_dir,
      enabled: selectedMcpServer.config.enabled
    };
    setMcpDraft(nextDraft);
    setStdioCommandLine(formatStdioCommandLine(nextDraft));
  }, [selectedMcpServer]);

  // Initial load
  useEffect(() => {
    if (restoredOnStartupRef.current) return;
    restoredOnStartupRef.current = true;
    void restoreMcpServers()
      .then((servers) => applyMcpServers(servers))
      .catch((error) => setNotice(`恢复 MCP 连接失败：${String(error)}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refreshMcpServers(selectId?: string) {
    try {
      const servers = await listMcpServers();
      applyMcpServers(servers, selectId);
    } catch (error) {
      setNotice(`加载 MCP 配置失败：${String(error)}`);
    }
  }

  function updateMcpServerView(view: McpServerView) {
    setMcpServers((current) => {
      const exists = current.some((server) => server.config.id === view.config.id);
      if (!exists) return [view, ...current];
      return current.map((server) => (server.config.id === view.config.id ? view : server));
    });
  }

  function handleNewMcpServer() {
    setSelectedMcpServerId("");
    setMcpDraft(emptyMcpDraft);
    setStdioCommandLine(formatStdioCommandLine(emptyMcpDraft));
  }

  function applyMcpServers(servers: McpServerView[], selectId?: string) {
    setMcpServers(servers);
    setSelectedMcpServerId((current) => {
      if (selectId && servers.some((server) => server.config.id === selectId)) {
        return selectId;
      }
      if (current && servers.some((server) => server.config.id === current)) {
        return current;
      }
      return servers[0]?.config.id || "";
    });
  }

  function getDraftWithStdioCommandLine() {
    if (mcpDraft.transport !== "stdio") {
      return {
        ...mcpDraft,
        command: "",
        args_json: "[]",
        env_json: "{}",
        working_dir: ""
      };
    }
    const stdioCommand = stdioCommandLine.trim()
      ? parseStdioCommandLine(stdioCommandLine)
      : { command: mcpDraft.command, args: parseJsonStringArray(mcpDraft.args_json) };
    return {
      ...mcpDraft,
      command: stdioCommand.command,
      args_json: JSON.stringify(stdioCommand.args),
      url: "",
      headers_json: "{}"
    };
  }

  async function handleSaveMcpServer() {
    try {
      const draftForSave = getDraftWithStdioCommandLine();
      const saved = await saveMcpServer({
        ...draftForSave,
        enabled: draftForSave.enabled
      });
      await refreshMcpServers(saved.id);
      setNotice("MCP 服务器配置已保存。");
    } catch (error) {
      setNotice(`保存 MCP 服务器失败：${String(error)}`);
    }
  }

  async function handleDeleteMcpServer() {
    if (!mcpDraft.id) {
      handleNewMcpServer();
      return;
    }
    if (!(await confirmAction("确定要删除该 MCP 服务器配置吗？"))) {
      return;
    }
    setMcpBusyId(mcpDraft.id);
    try {
      await deleteMcpServer(mcpDraft.id);
      await refreshMcpServers();
      setNotice("MCP 服务器已删除。");
    } catch (error) {
      setNotice(`删除 MCP 服务器失败：${String(error)}`);
    } finally {
      setMcpBusyId("");
    }
  }

  async function handleConnectMcpServer(id: string) {
    setMcpBusyId(id);
    try {
      const view = await connectMcpServer(id);
      updateMcpServerView(view);
      setSelectedMcpServerId(id);
      setNotice(`MCP 服务器 ${view.config.name} 已连接，发现 ${view.tools.length} 个工具。`);
    } catch (error) {
      await refreshMcpServers(id);
      setNotice(`连接 MCP 服务器失败：${String(error)}`);
    } finally {
      setMcpBusyId("");
    }
  }

  async function handleDisconnectMcpServer(id: string) {
    setMcpBusyId(id);
    try {
      await disconnectMcpServer(id);
      await refreshMcpServers(id);
      setNotice("MCP 服务器已断开。");
    } catch (error) {
      setNotice(`断开 MCP 服务器失败：${String(error)}`);
    } finally {
      setMcpBusyId("");
    }
  }

  async function handleRefreshMcpTools(id: string) {
    setMcpBusyId(id);
    try {
      const tools = await refreshMcpTools(id);
      setMcpServers((current) =>
        current.map((server) =>
          server.config.id === id
            ? {
                ...server,
                tools,
                status: {
                  ...server.status,
                  connected: true,
                  tool_count: tools.length,
                  error: null
                }
              }
            : server
        )
      );
      setNotice(`工具列表已刷新，共 ${tools.length} 个工具。`);
    } catch (error) {
      setNotice(`刷新 MCP 工具失败：${String(error)}`);
    } finally {
      setMcpBusyId("");
    }
  }

  return {
    mcpServers,
    setMcpServers,
    selectedMcpServerId,
    setSelectedMcpServerId,
    mcpDraft,
    setMcpDraft,
    stdioCommandLine,
    setStdioCommandLine,
    mcpBusyId,
    setMcpBusyId,
    selectedMcpServer,
    refreshMcpServers,
    updateMcpServerView,
    handleNewMcpServer,
    handleSaveMcpServer,
    handleDeleteMcpServer,
    handleConnectMcpServer,
    handleDisconnectMcpServer,
    handleRefreshMcpTools
  };
}
