import type { AgentRun, McpServerView, ProjectEntry } from "../../types";
import type { Skill } from "../../lib/skills";
import type { BackgroundAgentRequest } from "../../lib/backgroundAgent";
import { buildSystemMessage } from "../../lib/chatSystemMessage";
import { getConversationTaskBudget } from "../../lib/taskBudget";

export interface ExecutionRequestContext {
  skills: Skill[];
  mcpServers: McpServerView[];
  tempDir: string;
  accessMode: BackgroundAgentRequest["access_mode"];
}

export function buildExecutionRequest(
    context: ExecutionRequestContext,
    run: AgentRun,
    project: ProjectEntry | null,
    modelId: string,
    replaceMessageId: string | null = null
  ): BackgroundAgentRequest {
    const { skills, mcpServers, tempDir, accessMode } = context;
    const boundProject: ProjectEntry | null = run.project_path
      ? project?.path === run.project_path
        ? project
        : { id: run.project_path, name: run.project_path, path: run.project_path, opened_at: "" }
      : null;
    return {
      run_id: run.id,
      conversation_id: run.conversation_id,
      model_config_id: run.model_config_id || modelId,
      project_path: run.project_path || tempDir,
      system_message: buildSystemMessage(
        [],
        null,
        boundProject,
        [],
        skills,
        mcpServers,
        [],
        [],
        [],
        tempDir,
        true
      ),
      access_mode: accessMode,
      allow_command: skills.some((skill) => skill.id === "bash_tool" && skill.enabled),
      budget: getConversationTaskBudget(run.conversation_id),
      replace_message_id: replaceMessageId
    };
  }
