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
  tempDir?: string
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
          : "- 当前项目文件列表为空，或暂时无法读取。"
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

  const toolsSystemInstruction = `
如果你需要使用已启用的技能来执行操作（如读取文件、写入文件或执行命令），你必须在回答中输出以下格式的 XML 标签来发出工具调用请求：

1. 写入文件（如果文件不存在会自动创建并写入，用于创建或修改文件）：
<tool_call name="write_file">
  <path>文件名或路径（如 a.txt）</path>
  <content>写入的完整文件内容</content>
</tool_call>

2. 读取文件（读取本地文件内容）：
<tool_call name="read_file">
  <path>文件名或路径</path>
</tool_call>

3. 执行命令（对应 Bash Tool，仅在已启用 Bash Tool 时可用；Windows 下会根据命令语法自动识别 PowerShell 或 cmd 执行）：
<tool_call name="execute_command">
  <command>具体的终端命令行，例如 PowerShell: Get-ChildItem 或 cmd: dir /b</command>
</tool_call>

4. 图片 OCR（对应 PaddleOCR PP-OCRv6 Small，仅处理当前项目内图片路径）：
<tool_call name="ocr_image">
  <path>项目内图片相对路径，如 screenshots/page.png</path>
  <output_format>text</output_format>
</tool_call>

5. 调用 MCP 工具（仅限上方列出的已连接 MCP 工具）：
<tool_call name="mcp__server_id__tool_name">
  <arguments>{"key":"value"}</arguments>
</tool_call>

注意：一次仅发出一个工具调用，等待执行结果回传后，再继续后续思考或操作。`;

  const clarificationSystemInstruction = `当继续任务所需的信息缺失，而且不同选择会显著改变结果时，不要只在普通文本中提问。请输出一个结构化澄清请求：
<clarification>{"questions":[{"id":"stable_question_id","prompt":"需要用户确认的问题","options":[{"id":"recommended","label":"推荐选项","description":"选择后的影响","recommended":true},{"id":"alternative","label":"其他选项","description":"选择后的影响","recommended":false}],"allow_custom":true}]}</clarification>

澄清规则：
- 一次包含 1 到 3 个真正必要的问题，每题提供 2 到 5 个互斥选项。
- 每题优先标记一个 recommended 选项，供自动执行模式代替用户选择。
- id 必须简短、稳定且在当前请求内唯一；不要把澄清请求和工具调用放在同一次回答中。
- 如果根据现有上下文可以安全、合理地继续，就直接继续，不要为了确认显而易见的细节而澄清。`;

  const sections = [
    runtimeContext,
    profileContext || "",
    projectContext,
    mcpContext,
    skillsContext || mcpContext ? `当前已启用的技能列表与工具调用规范：\n${skillsContext || "无已启用本地技能"}\n\n${toolsSystemInstruction}` : "",
    clarificationSystemInstruction,
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
