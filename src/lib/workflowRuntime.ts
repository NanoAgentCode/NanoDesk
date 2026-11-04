import { listen } from '@tauri-apps/api/event';
import { chatStream, interruptChatStream } from '../api/chat';
import { approveAgentToolCall, createAgentRun, createAgentToolCall, executeAgentToolCall, finishAgentRun, rejectAgentToolCall, resolveAgentToolApproval } from '../api/agents';
import { archiveConversation, createConversation } from '../api/conversations';
import { confirmAction } from './dialogs';
import { ACCESS_MODE_STORAGE_KEY, parseAccessMode } from './accessMode';
import type { WorkflowExecutor } from './workflow';

export function workflowAI(modelId: string, prompt: string, signal: AbortSignal): Promise<string> {
  if (!modelId) return Promise.reject(new Error('请先选择模型'));
  return new Promise((resolve, reject) => {
    const id = crypto.randomUUID(); let output = ''; let settled = false; let unlisten: (() => void) | undefined;
    const finish = (error?: unknown) => { if (settled) return; settled = true; unlisten?.(); signal.removeEventListener('abort', abort); error ? reject(error) : resolve(output); };
    const abort = () => { void interruptChatStream(id).catch(() => {}); finish(new Error('运行已停止')); };
    void listen<{ request_id: string; type: string; content?: string; message?: string }>('chat-stream', ({ payload: e }) => {
      if (e.request_id !== id) return;
      if (e.type === 'delta') output += e.content ?? '';
      if (e.type === 'error') finish(new Error(e.message));
      if (e.type === 'interrupted') finish(new Error('运行已停止'));
    }).then(async off => {
      unlisten = off;
      if (signal.aborted) { off(); finish(new Error('运行已停止')); return; }
      signal.addEventListener('abort', abort, { once: true });
      try { await chatStream(id, modelId, [{ role: 'user', content: prompt }]); finish(); }
      catch (e) { finish(e); }
    }).catch(finish);
  });
}

export function createWorkflowExecutor(projectPath: string, allowCommand: boolean): WorkflowExecutor {
  let conversationId: Promise<string> | undefined;
  let approvalQueue = Promise.resolve();
  const confirm = (content: string, signal: AbortSignal): Promise<boolean> => {
    const answer = approvalQueue.then(() => signal.aborted ? false : confirmAction(content));
    approvalQueue = answer.then(() => {}, () => {});
    return answer;
  };
  return {
    ai: workflowAI,
    tool: async (name, args, signal) => {
      if (signal.aborted) throw new Error('运行已停止');
      if (!projectPath.trim()) throw new Error('工具节点需要设置项目目录');
      conversationId ??= createConversation({ title: '工作流工具执行记录', project_path: projectPath }).then(async conversation => {
        await archiveConversation(conversation.id, true); return conversation.id;
      });
      const run = await createAgentRun({ conversation_id: await conversationId, project_path: projectPath });
      try {
        const call = await createAgentToolCall({ run_id: run.id, message_id: crypto.randomUUID(), name, args_json: JSON.stringify(args) });
        const approval = await resolveAgentToolApproval({ tool_call_id: call.id, project_path: projectPath, allow_command: allowCommand, access_mode: parseAccessMode(localStorage.getItem(ACCESS_MODE_STORAGE_KEY)) });
        if (approval.requires_user_approval) {
          const accepted = await confirm(`工作流请求执行 ${name}\n${approval.reason}\n${JSON.stringify(args, null, 2)}`, signal);
          if (!accepted || signal.aborted) { await rejectAgentToolCall(call.id, '用户拒绝或停止运行'); throw new Error('工具执行已取消'); }
          await approveAgentToolCall(call.id);
        }
        if (signal.aborted) throw new Error('运行已停止');
        const result = await executeAgentToolCall({ tool_call_id: call.id, project_path: projectPath, allow_command: allowCommand });
        if (result.tool_call.status !== 'completed' && result.tool_call.status !== 'success') throw new Error(result.tool_call.error || result.result_text || '工具执行失败');
        await finishAgentRun(run.id, 'completed');
        return result.result_text;
      } catch (e) { await finishAgentRun(run.id, 'failed', String(e)); throw e; }
    }
  };
}
