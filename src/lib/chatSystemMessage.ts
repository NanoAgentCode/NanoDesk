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
  content: "你是一个专注的本地效率助手。请保持回答简明且实用。记忆库写入由应用本地功能处理；除非应用明确提供结果，否则不要声称已经保存或更新记忆。"
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
        `- 项目显示名称：${activeProject.name}（仅用于应用界面展示，不代表目录名，也不要用于拼接路径）`,
        `- 真实工作目录：${activeProject.path}`,
        "- 所有文件、命令、读写操作必须以真实工作目录为准；不要根据项目显示名称推断或追加子目录。",
        "- 如果你生成、编辑或展示图片、HTML、PDF、表格、文档等静态资源，请在回答中使用 Markdown 链接指向项目内真实相对路径（如 [预览图](screenshots/page.png)）；可以预览的文件仍要保留预览入口或预览说明。",
        projectFiles.length > 0
          ? `- 当前项目文件列表（最多 300 项，已跳过 node_modules、.git、target、dist 等大目录）：\n${formatProjectFileTree(projectFiles)}`
          : deferRetrieval ? "" : "- 当前项目文件列表为空，或暂时无法读取。"
      ].join("\n")
    : "";

  const enabledSkills = skills.filter((s) => s.enabled);
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

  const clarificationSystemInstruction = `当继续任务所需的信息缺失，而且不同选择会显著改变结果时，不要只在普通文本中提问。请输出一个结构化澄清请求：
<clarification>{"questions":[{"id":"stable_question_id","prompt":"需要用户确认的问题","options":[{"id":"recommended","label":"推荐选项","description":"选择后的影响","recommended":true},{"id":"alternative","label":"其他选项","description":"选择后的影响","recommended":false}],"allow_custom":true}]}</clarification>

澄清规则：
- 一次包含 1 到 3 个真正必要的问题，每题提供 2 到 5 个互斥选项。
- 每题优先标记一个 recommended 选项，供自动执行模式代替用户选择。
- id 必须简短、稳定且在当前请求内唯一；不要把澄清请求和工具调用放在同一次回答中。
- 如果根据现有上下文可以安全、合理地继续，就直接继续，不要为了确认显而易见的细节而澄清。`;

  const planningSystemInstruction = `每次新任务都先生成结构化任务规划。请求批准模式下，首次只规划，所有步骤为 pending，等待用户审核后再执行：
<task_plan>{"goal":"简洁且可验证的目标","steps":[{"id":"inspect","title":"检查现状","status":"in_progress"},{"id":"implement","title":"实施修改","status":"pending"},{"id":"verify","title":"运行验证","status":"pending"}]}</task_plan>

计划规则：
- 计划包含 2 到 12 个简洁步骤，id 在本次任务中保持稳定且唯一。
- status 只能是 pending、in_progress、completed、blocked、skipped；同时最多一个 in_progress。
- 每次获得工具结果后更新计划。只有得到实际证据后才能标记 completed；真正无法继续时才标记 blocked。
- 请求批准模式下，未审核或变更目标、步骤内容时，只输出规划并等待用户审核；审核通过后才可调用工具。其他模式可与 tool_call 同时输出。不要在普通正文里重复整份计划。`;

  const sections = [
    runtimeContext,
    profileContext || "",
    projectContext,
    activeProject && !deferRetrieval ? "代码定位优先使用当前问题的代码索引检索结果，先按命中路径和行号读取文件。索引不足或过期时，可在明确的项目子目录内搜索（例如 rg -n pattern src/components，或 Get-ChildItem -Path src/components -Recurse -Filter *.tsx）；不要从项目根目录递归扫描。没有索引时提示使用左侧项目名称旁的数据库图标建立项目索引，但仍可读取已知文件或执行限定目录的搜索。工具被拒绝后应缩小范围，不要原样重复失败命令。" : "",
    mcpContext,
    skillsContext || mcpContext ? `当前已启用的技能列表与工具调用规范：\n${skillsContext || "无已启用本地技能"}\n\n${toolsSystemInstruction}` : "",
    clarificationSystemInstruction,
    deferRetrieval ? "" : planningSystemInstruction,
    memoryContext ? [
      "用户个性化记忆（用于保持跨会话一致性）：",
      "- 这些记忆可能包含用户偏好、身份背景、工作方式、常用技术栈或长期项目上下文。",
      "- 回答时优先尊重明确的用户偏好；如果记忆与当前请求无关，不要刻意提及。",
      "- 如果当前消息与旧记忆冲突，以当前消息为准，不要争辩旧记忆。",
      memoryContext
    ].join("\n") : "",
    codeContext ? [
      "当前项目代码索引检索结果，仅在回答代码结构、调用链、实现位置或修改建议时使用：",
      "- 回答代码问题时优先引用这些路径和行号；如果结果不足，请说明需要读取更精确的文件内容。",
      codeContext
    ].join("\n") : "",
    projectIndexContext ? [
      "当前项目文档索引检索结果，用于回答项目说明、配置、数据文件、文档和普通文件内容相关问题：",
      "- 优先引用这些项目内路径和行号；如果结果不足，再请求读取更精确的文件内容。",
      projectIndexContext
    ].join("\n") : "",
    ragContext ? `当前对话上传文件检索结果，仅在回答当前问题相关时使用：\n${ragContext}` : ""
  ].filter(Boolean);

  return {
    role: "system",
    content: `${systemMessage.content}\n\n${sections.join("\n\n")}`
  };
}
