import { buildRuntimeContext, formatProjectFileTree } from "./formatters";
import type {
  ChatMessage,
  CodeSearchResult,
  Memory,
  McpServerView,
  ProjectEntry,
  ProjectFileEntry,
  ProjectIndexSearchResult,
  RagChunkMatch
} from "../types";

export const systemMessage: ChatMessage = {
  role: "system",
  content: "你是本地效率助手，回答简明实用。记忆由应用写入；无应用成功结果，不得声称已保存或更新记忆。"
};

export function buildSystemMessage(
  memories: Memory[],
  profileContext: string | null = null,
  activeProject: ProjectEntry | null = null,
  projectFiles: ProjectFileEntry[] = [],
  skills: any[] = [],
  mcpServers: McpServerView[] = [],
  ragMatches: RagChunkMatch[] = [],
  codeMatches: CodeSearchResult[] = [],
  projectIndexMatches: ProjectIndexSearchResult[] = [],
  tempDir?: string,
  deferRetrieval = false
): ChatMessage {
  const runtimeContext = buildRuntimeContext();

  const memoryContext = memories
    .map((memory) => {
      const tags = memory.tags.length > 0 ? ` [${memory.tags.join(", ")}]` : "";
      return `- ${memory.title}${tags}: ${memory.content}`;
    })
    .join("\n");

  const ragContext = ragMatches.length > 0
    ? ragMatches
        .map((match, index) => {
          const score = Number.isFinite(match.score) ? match.score.toFixed(3) : "0.000";
          return `[${index + 1}] ${match.file_name} · chunk ${match.chunk_index + 1} · score ${score}\n${match.text}`;
        })
        .join("\n\n")
    : "";

  const codeContext = codeMatches.length > 0
    ? codeMatches
        .map((match, index) => {
          const score = Number.isFinite(match.score) ? match.score.toFixed(3) : "0.000";
          return `[${index + 1}] ${match.file_path}:${match.start_line}-${match.end_line} · ${match.kind} · ${match.name} · score ${score}\n${match.snippet}`;
        })
        .join("\n\n")
    : "";

  const projectIndexContext = projectIndexMatches.length > 0
    ? projectIndexMatches
        .map((match, index) => {
          const score = Number.isFinite(match.score) ? match.score.toFixed(3) : "0.000";
          return `[${index + 1}] ${match.file_path}:${match.start_line}-${match.end_line} · ${match.title} · ${match.indexer} · chunk ${match.chunk_index + 1} · score ${score}\n${match.snippet}`;
        })
        .join("\n\n")
    : "";

  const projectContext = activeProject
    ? [
        "当前项目上下文：",
        `- 显示名称：${activeProject.name}（非目录名，不用于拼接路径）`,
        `- 真实工作目录：${activeProject.path}`,
        "- 文件读写与命令均以真实工作目录为准，不按显示名称追加子目录。",
        "- 生成、编辑或展示静态资源须提供 Markdown 真实相对路径链接，如 [预览图](screenshots/page.png)；可预览文件须保留预览入口或说明。",
        projectFiles.length > 0
          ? `- 当前项目文件列表（最多 300 项，已跳过 node_modules、.git、target、dist 等大目录）：\n${formatProjectFileTree(projectFiles)}`
          : deferRetrieval ? "" : "- 当前项目文件列表为空，或暂时无法读取。"
      ].join("\n")
    : tempDir
      ? [
          `对话工作目录：${tempDir}`,
          "- 未绑定项目，支持在此目录内读取和写入文件，无需创建项目。",
          "- read_file、write_file 的 path 必须相对此目录，例如 <path>output/report.md</path>；不要拼接工作目录、使用盘符或 ..。命令也在此目录执行。",
          "- 生成文件后使用 Markdown 相对链接提供入口，例如 [查看报告](output/report.md)，只有工具返回成功后才声称已写入。"
        ].join("\n")
      : "";

  const enabledSkills = skills.filter((s) => s.enabled);
  const commandGenerationInstruction = enabledSkills.some((s) => s.id === "bash_tool")
    ? [
        "命令生成规则：",
        "- 参数以本机 CLI 帮助为准；不要将 API 字段名直接当作 CLI 参数或套用其他工具、旧版选项。",
        "- 缺少帮助、参数存疑或技能与本机行为冲突时，先单独调用该子命令的 --help，等待实际帮助结果后生成业务命令；两者不得串用，复用本轮已有帮助。",
        "- 遵循本机 shell 语法，正确引用空格、中文和特殊字符；命令简短，仅用必需且确认支持的参数。",
        "- 选项或参数值报错时不要原样重试；先查子命令帮助，再生成完整调用。重试前检查是否已部分执行，并遵守审批策略。",
        enabledSkills.some((s) => s.id === "tavily_search" || s.id === "tavily_cli")
          ? "- Tavily：首次先读 tvly search --help。已验证：--time-range day|week|month|year（只选一值），或 --start-date YYYY-MM-DD 与 --end-date YYYY-MM-DD；不支持 --days。任意天数按运行上下文计算日期，不改成近一周；以本机帮助为准。"
          : ""
      ].filter(Boolean).join("\n")
    : "";
  const mcpTools = mcpServers.flatMap((server) =>
    server.status.connected
      ? server.tools.map((tool) => ({
          callName: `mcp__${tool.server_id}__${tool.name}`,
          serverName: server.config.name,
          tool
        }))
      : []
  );
  const mcpContext = mcpTools.length > 0
    ? [
        "当前已连接的 MCP 工具：",
        ...mcpTools.map(({ callName, serverName, tool }) =>
          `- ${callName}\n  服务器：${serverName}\n  描述：${tool.description || "无"}\n  参数 schema：${tool.input_schema_json}`
        )
      ].join("\n")
    : "";
  const skillsContext = enabledSkills.length > 0
    ? [
        "当前已启用的本地/系统技能（Skills）：",
        ...enabledSkills.map((s) => {
          const parameters = { ...s.parameters };
          if (activeProject?.path) {
            if ("workspace_root" in parameters) {
              parameters.workspace_root = activeProject.path;
            }
            if ("output_dir" in parameters) {
              parameters.output_dir = activeProject.path;
            }
            if ("skills_root" in parameters) {
              parameters.skills_root = activeProject.path + "\\.agents\\skills";
            }
          } else if (tempDir) {
            if ("workspace_root" in parameters) {
              parameters.workspace_root = tempDir;
            }
            if ("output_dir" in parameters) {
              parameters.output_dir = tempDir;
            }
          }

          const params = parameters && Object.keys(parameters).length > 0
            ? "\n  自动运行参数：\n" + Object.entries(parameters)
                .map(([k, v]) => `    * ${k}: ${v}`)
                .join("\n")
            : "";
          return `- 名称：${s.name} (ID: ${s.id})\n  提供者：${s.provider}\n  描述：${s.description}${params}`;
        })
      ].join("\n")
    : "";

  const toolsSystemInstruction = `工具调用使用 XML，每次只调用一个，等待结果后继续；只能调用已启用工具：
写文件：<tool_call name="write_file"><path>路径</path><content>完整内容</content></tool_call>
读文件：<tool_call name="read_file"><path>路径</path></tool_call>
命令（需启用 Bash Tool，Windows 自动识别 PowerShell/cmd）：<tool_call name="execute_command"><command>命令</command></tool_call>
OCR（仅项目内图片）：<tool_call name="ocr_image"><path>相对路径</path><output_format>text</output_format></tool_call>
MCP（仅已连接工具，参数遵循 schema）：<tool_call name="mcp__server_id__tool_name"><arguments>{"key":"value"}</arguments></tool_call>`;

  const clarificationSystemInstruction = `仅当缺失信息会显著改变结果时，用以下结构澄清，不只在正文提问；可合理继续时直接执行：
<clarification>{"questions":[{"id":"stable_question_id","prompt":"需要用户确认的问题","options":[{"id":"recommended","label":"推荐选项","description":"选择后的影响","recommended":true},{"id":"alternative","label":"其他选项","description":"选择后的影响","recommended":false}],"allow_custom":true}]}</clarification>

一次包含 1 到 3 个真正必要的问题，每题 2 到 5 个互斥选项，优先标记一个 recommended 供自动模式选择。id 简短、稳定且请求内唯一；澄清与工具调用不得同轮输出。`;

  const planningSystemInstruction = `每次新任务都先生成结构化任务规划。请求批准模式下，首次只规划，所有步骤为 pending，等待用户审核后再执行：
<task_plan>{"goal":"可验证目标","steps":[{"id":"inspect","title":"检查","status":"pending"},{"id":"verify","title":"验证","status":"pending"}]}</task_plan>

2 到 12 步，id 稳定唯一；status 为 pending/in_progress/completed/blocked/skipped，最多一个 in_progress。每次工具结果后更新，有证据才 completed，确实无法继续才 blocked。请求批准模式未审核或目标、步骤变更时只输出规划，审核后才调用工具；其他模式可与 tool_call 同轮输出。正文不重复计划。`;

  const sections = [
    runtimeContext,
    profileContext || "",
    projectContext,
    activeProject && !deferRetrieval ? "代码定位优先使用当前问题的代码索引检索结果，先按命中路径和行号读取文件。索引不足或过期时，可在明确的项目目录内搜索，允许显式指定当前项目根目录（例如 rg -n pattern .，或 Get-ChildItem -Path . -Recurse -Filter *.tsx），优先缩小到相关子目录。禁止搜索父目录、项目外或范围不明的路径。没有索引时提示使用左侧项目名称旁的数据库图标建立项目索引，但仍可读取已知文件或执行项目内搜索。工具被拒绝后应缩小范围，不要原样重复失败命令。" : "",
    mcpContext,
    commandGenerationInstruction,
    skillsContext || mcpContext ? `当前已启用的技能列表与工具调用规范：\n${skillsContext || "无已启用本地技能"}\n\n${toolsSystemInstruction}` : "",
    clarificationSystemInstruction,
    deferRetrieval ? "" : planningSystemInstruction,
    memoryContext ? [
      "用户记忆：尊重相关偏好，无关时不提及；冲突以当前消息为准。",
      memoryContext
    ].join("\n") : "",
    codeContext ? [
      "代码索引（仅用于代码问题）：优先引用路径和行号，不足时说明需读取精确内容。",
      codeContext
    ].join("\n") : "",
    projectIndexContext ? [
      "项目文档索引（说明、配置、数据及普通文件）：优先引用路径和行号，不足时请求读取精确内容。",
      projectIndexContext
    ].join("\n") : "",
    ragContext ? `当前对话上传文件检索结果，仅在回答当前问题相关时使用：\n${ragContext}` : ""
  ].filter(Boolean);

  return {
    role: "system",
    content: `${systemMessage.content}\n\n${sections.join("\n\n")}`
  };
}
