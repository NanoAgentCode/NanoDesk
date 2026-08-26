import { AlertTriangle, Info, Loader2, PlugZap, Plus, RotateCcw, Save, Trash2, Unplug } from "lucide-react";
import {
  Alert,
  Select,
  Textarea,
  TextInput
} from "@mantine/core";
import IconTooltipButton from "../IconTooltipButton";
import { formatMcpTransportLabel } from "../../lib/formatters";
import type { UseMcpReturn } from "../../hooks/useMcp";
import type { McpServerDraft } from "../../types";

interface SettingsMcpTabProps {
  mcp: UseMcpReturn;
}

type JsonObjectDraftField = "env_json" | "headers_json";

function readObjectEntries(value: string): Array<[string, string]> {
  try {
    const parsed = JSON.parse(value || "{}") as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return [];
    return Object.entries(parsed).map(([key, entryValue]) => [key, String(entryValue)]);
  } catch {
    return [];
  }
}

function entriesToJson(entries: Array<[string, string]>) {
  const nextValue = Object.fromEntries(entries.map(([key, value]) => [key.trim(), value]));
  return JSON.stringify(nextValue);
}

function updateObjectEntry(
  draft: McpServerDraft,
  field: JsonObjectDraftField,
  index: number,
  nextKey: string,
  nextValue: string
): McpServerDraft {
  const entries = readObjectEntries(draft[field]);
  entries[index] = [nextKey, nextValue];
  return { ...draft, [field]: entriesToJson(entries) };
}

function removeObjectEntry(draft: McpServerDraft, field: JsonObjectDraftField, index: number): McpServerDraft {
  const entries = readObjectEntries(draft[field]);
  entries.splice(index, 1);
  return { ...draft, [field]: entriesToJson(entries) };
}

function addObjectEntry(draft: McpServerDraft, field: JsonObjectDraftField): McpServerDraft {
  const entries = readObjectEntries(draft[field]);
  const baseKey = field === "env_json" ? "API_KEY" : "Authorization";
  const existingKeys = new Set(entries.map(([key]) => key));
  const nextKey = existingKeys.has(baseKey) ? `${baseKey}_${entries.length + 1}` : baseKey;
  return { ...draft, [field]: entriesToJson([...entries, [nextKey, ""]]) };
}

export default function SettingsMcpTab({ mcp }: SettingsMcpTabProps) {
  const envEntries = readObjectEntries(mcp.mcpDraft.env_json);
  const headerEntries = readObjectEntries(mcp.mcpDraft.headers_json);

  function renderObjectField(label: string, field: JsonObjectDraftField, entries: Array<[string, string]>) {
    return (
      <div className="mcp-kv-field">
        <div className="mcp-kv-field-header">
          <span>{label}</span>
          <IconTooltipButton className="settings-icon-compact" label={`添加${label}`} onClick={() => mcp.setMcpDraft(addObjectEntry(mcp.mcpDraft, field))}><Plus size={15} /></IconTooltipButton>
        </div>
        <div className="mcp-kv-list">
          {entries.map(([key, value], index) => (
            <div className="mcp-kv-row" key={`${field}-${index}`}>
              <TextInput size="xs" value={key} onChange={(event) => mcp.setMcpDraft(updateObjectEntry(mcp.mcpDraft, field, index, event.currentTarget.value, value))} placeholder="名称" />
              <TextInput size="xs" value={value} onChange={(event) => mcp.setMcpDraft(updateObjectEntry(mcp.mcpDraft, field, index, key, event.currentTarget.value))} placeholder="值" />
              <IconTooltipButton className="settings-icon-compact" label={`删除${label}`} tone="danger" onClick={() => mcp.setMcpDraft(removeObjectEntry(mcp.mcpDraft, field, index))}><Trash2 size={15} /></IconTooltipButton>
            </div>
          ))}
          {entries.length === 0 && <div className="mcp-kv-empty">未配置</div>}
        </div>
      </div>
    );
  }

  return (
    <div className="settings-tab-content model-tab-content">
      <div className="model-header-row">
        <h3>MCP 配置</h3>
        <IconTooltipButton label="添加 MCP 服务器" onClick={mcp.handleNewMcpServer}><Plus size={18} /></IconTooltipButton>
      </div>
      <p className="description description--tight">连接符合 Model Context Protocol 规范的工具服务器，支持 stdio、SSE 和 Streamable HTTP。</p>
      <div className="model-config-grid mcp-config-grid">
        <aside className="model-config-list">
          {mcp.mcpServers.map((server) => {
            const connected = server.status.connected;
            const busy = mcp.mcpBusyId === server.config.id;
            return (
              <button key={server.config.id} className={server.config.id === mcp.selectedMcpServerId ? "mcp-config-row active" : "mcp-config-row"}
                onClick={() => mcp.setSelectedMcpServerId(server.config.id)} type="button">
                <div className="mcp-config-row-header">
                  <strong>{server.config.name}</strong>
                  <IconTooltipButton className="settings-icon-compact"
                    label={busy ? "处理中" : connected ? "断开 MCP 服务器" : "连接 MCP 服务器"}
                    tone={connected ? "success" : "default"}
                    onClick={(event) => { event.stopPropagation(); if (connected) { void mcp.handleDisconnectMcpServer(server.config.id); } else { void mcp.handleConnectMcpServer(server.config.id); } }}
                    disabled={busy}>
                    {busy ? <Loader2 size={15} className="svg-spin mcp-loader-small" /> : connected ? <Unplug size={15} /> : <PlugZap size={15} />}
                  </IconTooltipButton>
                </div>
                <span title={server.config.command || server.config.url}>{formatMcpTransportLabel(server.config.transport)} · {server.config.command || server.config.url} · {server.tools.length} tools</span>
                {server.status.error && (
                  <span className="mcp-row-error" title={server.status.error}>启动失败</span>
                )}
              </button>
            );
          })}
          {mcp.mcpServers.length === 0 && <div className="empty">暂无 MCP 服务器配置</div>}
        </aside>
        <div className="model-config-form">
          <div className="model-form-card mcp-form-card">
            <TextInput label="服务名称" value={mcp.mcpDraft.name} onChange={(event) => mcp.setMcpDraft({ ...mcp.mcpDraft, name: event.currentTarget.value })} placeholder="amap-maps" />
            <Select
              label="协议"
              value={mcp.mcpDraft.transport}
              data={[
                { value: "stdio", label: "stdio 本地进程" },
                { value: "sse", label: "SSE" },
                { value: "streamable_http", label: "Streamable HTTP" }
              ]}
              onChange={(value) => value && mcp.setMcpDraft({ ...mcp.mcpDraft, transport: value })}
              allowDeselect={false}
            />
            {mcp.mcpDraft.transport === "stdio" ? (
              <>
                <Textarea label="命令" value={mcp.stdioCommandLine} onChange={(event) => mcp.setStdioCommandLine(event.currentTarget.value)} rows={2}
                  placeholder={"npx -y @modelcontextprotocol/server-filesystem C:\\Users\\13439\\Desktop"} spellCheck={false} />
                {renderObjectField("环境变量", "env_json", envEntries)}
                <TextInput label="工作目录" value={mcp.mcpDraft.working_dir} onChange={(event) => mcp.setMcpDraft({ ...mcp.mcpDraft, working_dir: event.currentTarget.value })} placeholder="可选" />
              </>
            ) : (
              <>
                <TextInput label="地址" value={mcp.mcpDraft.url} onChange={(event) => mcp.setMcpDraft({ ...mcp.mcpDraft, url: event.currentTarget.value })}
                  placeholder={mcp.mcpDraft.transport === "sse" ? "https://example.com/sse" : "https://example.com/mcp"} />
                {renderObjectField("请求头", "headers_json", headerEntries)}
              </>
            )}
            {mcp.selectedMcpServer?.status.error && (
              <Alert className="mcp-error-panel" color="red" title="启动诊断" icon={<AlertTriangle />} role="status">
                <pre>{mcp.selectedMcpServer.status.error}</pre>
              </Alert>
            )}
          </div>
          <div className="modal-actions icon-actions mcp-actions icon-actions-bar">
            <div className="mcp-action-status">
              {mcp.selectedMcpServer?.status.error && (
                <span className="mcp-status-text error" title={mcp.selectedMcpServer.status.error}>启动失败</span>
              )}
              {mcp.selectedMcpServer && (
                <div className="mcp-tools-tooltip-wrap">
                  <IconTooltipButton className="settings-icon-compact" label="查看工具详情"><Info size={15} /></IconTooltipButton>
                  <div className="mcp-tools-tooltip" role="tooltip">
                    <div className="mcp-tools-tooltip-header">
                      <strong>工具详情{mcp.selectedMcpServer.status.connected ? ` · ${mcp.selectedMcpServer.tools.length}` : ""}</strong>
                      {mcp.selectedMcpServer.status.connected && (
                        <IconTooltipButton className="settings-icon-compact" label={mcp.mcpBusyId === mcp.selectedMcpServer.config.id ? "刷新中" : "刷新工具列表"} onClick={() => void mcp.handleRefreshMcpTools(mcp.selectedMcpServer!.config.id)}
                          disabled={mcp.mcpBusyId === mcp.selectedMcpServer.config.id}>
                          {mcp.mcpBusyId === mcp.selectedMcpServer.config.id ? <Loader2 size={15} className="svg-spin" /> : <RotateCcw size={15} />}
                        </IconTooltipButton>
                      )}
                    </div>
                    {!mcp.selectedMcpServer.status.connected && <div className="mcp-tools-tooltip-empty">连接后可查看工具</div>}
                    {mcp.selectedMcpServer.status.connected && mcp.selectedMcpServer.tools.length === 0 && <div className="mcp-tools-tooltip-empty">该服务器暂未暴露工具</div>}
                    {mcp.selectedMcpServer.status.connected && mcp.selectedMcpServer.tools.length > 0 && (
                      <div className="mcp-tools-tooltip-list">
                        {mcp.selectedMcpServer.tools.map((tool) => (
                          <div key={`${tool.server_id}:${tool.name}`} className="mcp-tools-tooltip-item">
                            <strong>{tool.name}</strong>
                            {tool.description && <span>{tool.description}</span>}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
            <div className="profile-header-actions">
              <IconTooltipButton label="保存 MCP 配置" tone="success" onClick={mcp.handleSaveMcpServer}><Save size={18} /></IconTooltipButton>
              <IconTooltipButton label="删除 MCP 配置" tone="danger" onClick={mcp.handleDeleteMcpServer} disabled={mcp.mcpBusyId === mcp.mcpDraft.id}><Trash2 size={18} /></IconTooltipButton>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
