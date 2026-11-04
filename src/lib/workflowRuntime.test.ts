import { beforeEach, describe, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ invoke: vi.fn(), confirm: vi.fn(), listen: vi.fn(), callback: undefined as undefined | ((event: { payload: Record<string, string> }) => void), off: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mock.listen }));
vi.mock('./dialogs', () => ({ confirmAction: mock.confirm }));
import { createWorkflowExecutor, workflowAI } from './workflowRuntime';
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('localStorage', { getItem: () => 'ask' });
  mock.confirm.mockResolvedValue(true);
  mock.listen.mockImplementation(async (_name, cb) => { mock.callback = cb; return mock.off; });
  mock.invoke.mockImplementation(async (command, args) => {
    if (command === 'create_conversation') return { id: 'conversation' };
    if (command === 'create_agent_run') return { id: 'run' };
    if (command === 'create_agent_tool_call') return { id: 'call' };
    if (command === 'resolve_agent_tool_approval') return { requires_user_approval: true, reason: 'requires approval' };
    if (command === 'execute_agent_tool_call') return { tool_call: { status: 'completed' }, result_text: 'done' };
    if (command === 'chat_stream') mock.callback?.({ payload: { request_id: args.request.request_id, type: 'delta', content: 'answer' } });
  });
});
describe('workflow runtime adapters', () => {
  it('uses streaming model configuration and removes subscription', async () => {
    expect(await workflowAI('model', 'prompt', new AbortController().signal)).toBe('answer');
    expect(mock.invoke).toHaveBeenCalledWith('chat_stream', expect.objectContaining({ request: expect.objectContaining({ model_config_id: 'model', messages: [{ role: 'user', content: 'prompt' }] }) }));
    expect(mock.off).toHaveBeenCalledOnce();
  });
  it('interrupts an in-flight model request', async () => {
    mock.invoke.mockImplementation(async command => { if (command === 'chat_stream') await new Promise(() => {}); });
    const controller = new AbortController();
    const pending = workflowAI('model', 'prompt', controller.signal);
    await vi.waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('chat_stream', expect.anything()));
    controller.abort(); await expect(pending).rejects.toThrow('停止');
    expect(mock.invoke).toHaveBeenCalledWith('interrupt_chat_stream', expect.anything()); expect(mock.off).toHaveBeenCalledOnce();
  });
  it('uses policy approval before executing and archives audit conversation', async () => {
    expect(await createWorkflowExecutor('D:/project', false).tool('read_file', { path: 'a' }, new AbortController().signal)).toBe('done');
    const commands = mock.invoke.mock.calls.map(c => c[0]);
    expect(commands.indexOf('resolve_agent_tool_approval')).toBeLessThan(commands.indexOf('approve_agent_tool_call'));
    expect(commands.indexOf('approve_agent_tool_call')).toBeLessThan(commands.indexOf('execute_agent_tool_call'));
    expect(mock.invoke).toHaveBeenCalledWith('archive_conversation', { id: 'conversation', archived: true });
    expect(mock.invoke).toHaveBeenCalledWith('execute_agent_tool_call', { request: { tool_call_id: 'call', project_path: 'D:/project', allow_command: false } });
  });
  it('does not execute rejected tools', async () => {
    mock.confirm.mockResolvedValue(false);
    await expect(createWorkflowExecutor('D:/project', false).tool('write_file', {}, new AbortController().signal)).rejects.toThrow('取消');
    expect(mock.invoke.mock.calls.some(c => c[0] === 'execute_agent_tool_call')).toBe(false);
    expect(mock.invoke).toHaveBeenCalledWith('reject_agent_tool_call', expect.anything());
  });
  it('serializes parallel approvals', async () => {
    let release: ((v: boolean) => void) | undefined;
    mock.confirm.mockImplementationOnce(() => new Promise<boolean>(r => { release = r; }));
    const ex = createWorkflowExecutor('D:/project', false), signal = new AbortController().signal;
    const a = ex.tool('a', {}, signal), b = ex.tool('b', {}, signal);
    await vi.waitFor(() => expect(mock.confirm).toHaveBeenCalledTimes(1));
    release!(true); await Promise.all([a, b]); expect(mock.confirm).toHaveBeenCalledTimes(2);
  });
});
