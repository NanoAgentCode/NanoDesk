import type { AgentToolCall, PersistedMessage, ProjectFileEntry, RagFile } from "../types";

export interface Deliverable { id: string; name: string; path?: string; kind: "upload" | "intermediate" | "final" }
const normalize = (path: string) => path.replace(/\\/g, "/").replace(/^\.\//, "");

/** Only successful writes are generated files; assistant links identify delivery. */
export function collectDeliverables(conversationId: string, messages: PersistedMessage[], files: RagFile[], tools: AgentToolCall[], projectFiles: ProjectFileEntry[] = [], projectPath = ""): Deliverable[] {
  const result = new Map<string, Deliverable>();
  const scoped = messages.filter(message => message.conversation_id === conversationId);
  const messageIds = new Set(scoped.map(message => message.id));
  const add = (path: string, kind: Deliverable["kind"], name?: string) => {
    path = normalize(path);
    result.set(path, { id: path, path, name: name || path.split("/").pop() || path, kind });
  };
  for (const file of files.filter(file => file.conversation_id === conversationId))
    result.set(`upload:${file.id}`, { id: `upload:${file.id}`, name: file.name, kind: "upload" });
  for (const message of scoped.filter(message => message.role === "user")) {
    const block = message.content.split("图片附件：")[1]?.split("需要识别图片文字时")[0];
    for (const match of block?.matchAll(/^- (.+?): (.+)$/gm) || []) add(match[2].trim(), "upload", match[1]);
  }
  for (const tool of tools) {
    if (!messageIds.has(tool.message_id) || tool.name !== "write_file" || tool.status !== "completed") continue;
    try {
      const args = JSON.parse(tool.args_json);
      if (typeof args.path === "string" && args.path.trim()) add(args.path, "intermediate");
    } catch { /* Older malformed records must not hide other files. */ }
  }
  for (const message of scoped.filter(message => message.role === "assistant")) {
    const content = message.content.replace(/<tool_call\b[\s\S]*?<\/tool_call>/g, "");
    for (const match of content.matchAll(/\[[^\]]*\]\((?:<([^>]+)>|([^\s)]+))\)/g)) {
      let path = match[1] || match[2];
      try { path = decodeURIComponent(path); } catch { continue; }
      path = normalize(path);
      const root = normalize(projectPath).replace(/\/+$/, "");
      if (root && path.startsWith(`${root}/`)) path = path.slice(root.length + 1);
      const existing = result.get(path);
      if (existing?.kind === "intermediate") existing.kind = "final";
      else if (!existing && projectFiles.some(file => !file.is_dir && normalize(file.path) === path)) add(path, "final");
    }
  }
  return [...result.values()];
}
