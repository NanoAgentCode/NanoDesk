import { beforeEach, describe, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ invoke: vi.fn(), confirm: vi.fn(), listen: vi.fn(), callbacks: new Set<(event: { payload: Record<string, string> }) => void>(), responses: [] as string[], off: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mock.listen }));
vi.mock('./dialogs', () => ({ confirmAction: mock.confirm }));
import { createWorkflowExecutor, workflowAI } from './workflowRuntime';
const tool = JSON.stringify({ type: 'tool', name: 'read_file', args: { path: 'a.txt' } });
const final = JSON.stringify({ type: 'final', content: 'answer' });
beforeEach(() => {
  vi.clearAllMocks(); mock.callbacks.clear(); mock.responses = [];
  vi.stubGlobal('localStorage', { getItem: () => 'ask' });
  mock.confirm.mockResolvedValue(true);
  mock.listen.mockImplementation(async (_name, cb) => { mock.callbacks.add(cb); return () => { mock.callbacks.delete(cb); mock.off(); }; });
  mock.invoke.mockImplementation(async (command, args) => {
    if (command === 'list_agent_tool_definitions') return [{ name: 'read_file', description: 'Read file', parameters_json: '{}' }, { name: 'execute_command', description: 'Command', parameters_json: '{}' }];
    if (command === 'list_mcp_servers') return [];
    if (command === 'create_conversation') return { id: 'conversation' };
    if (command === 'create_agent_run') return { id: 'run' };
    if (command === 'create_agent_tool_call') return { id: 'call' };
    if (command === 'resolve_agent_tool_approval') return { requires_user_approval: true, reason: 'requires approval' };
    if (command === 'execute_agent_tool_call') return { tool_call: { status: 'completed' }, result_text: 'file data' };
    if (command === 'chat_stream') for (const cb of mock.callbacks) cb({ payload: { request_id: args.request.request_id, type: 'delta', content: mock.responses[0] ?? 'answer' } });
    if (command === 'chat_stream') mock.responses.shift();
  });
});
describe('workflow AI runtime', () => {
  it('sends separate system/user messages and removes subscription', async () => {
    expect(await workflowAI('model', 'task', new AbortController().signal, 'role')).toBe('answer');
    expect(mock.invoke).toHaveBeenCalledWith('chat_stream', expect.objectContaining({ request: expect.objectContaining({ messages: [{ role: 'system', content: 'role' }, { role: 'user', content: 'task' }] }) }));
    expect(mock.off).toHaveBeenCalledOnce();
  });
  it('allows an AI node to finish without calling any tool', async () => {
    mock.responses = [final];
    expect(await createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal, 'role')).toBe('answer');
    expect(mock.invoke.mock.calls.some(c => c[0] === 'create_agent_tool_call')).toBe(false);
    const system = mock.invoke.mock.calls.find(c => c[0] === 'chat_stream')![1].request.messages[0].content;
    expect(system).toContain('role'); expect(system).not.toContain('execute_command');
  });
  it('executes a model-selected tool under policy and passes result back before final answer', async () => {
    mock.responses = [tool, final];
    expect(await createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal)).toBe('answer');
    const commands = mock.invoke.mock.calls.map(c => c[0]);
    expect(commands.indexOf('resolve_agent_tool_approval')).toBeLessThan(commands.indexOf('approve_agent_tool_call'));
    expect(commands.indexOf('approve_agent_tool_call')).toBeLessThan(commands.indexOf('execute_agent_tool_call'));
    expect(mock.invoke).toHaveBeenCalledWith('execute_agent_tool_call', { request: { tool_call_id: 'call', project_path: 'D:/project', allow_command: false } });
    const last = mock.invoke.mock.calls.filter(c => c[0] === 'chat_stream').at(-1)![1].request.messages;
    expect(last.at(-1).content).toContain('file data');
  });
  it('does not execute rejected tools or claim success', async () => {
    mock.responses = [tool]; mock.confirm.mockResolvedValue(false);
    await expect(createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal)).rejects.toThrow('取消');
    expect(mock.invoke.mock.calls.some(c => c[0] === 'execute_agent_tool_call')).toBe(false);
  });
  it('rejects invented tool names', async () => {
    mock.responses = [JSON.stringify({ type: 'tool', name: 'invented', args: {} })];
    await expect(createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal)).rejects.toThrow('不可用');
  });
  it('enforces the tool-call limit without executing an extra tool', async () => {
    mock.responses = Array(13).fill(tool);
    await expect(createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal)).rejects.toThrow('12 次');
    expect(mock.invoke.mock.calls.filter(c => c[0] === 'execute_agent_tool_call')).toHaveLength(12);
  });
  it('does not invoke tools without a project directory', async () => {
    expect(await createWorkflowExecutor('', false).ai('model', 'task', new AbortController().signal, 'role')).toBe('answer');
    expect(mock.invoke.mock.calls.some(c => c[0] === 'list_agent_tool_definitions')).toBe(false);
  });
  it('lets the model select connected MCP tools through the same approval path', async () => {
    const base = mock.invoke.getMockImplementation()!;
    mock.invoke.mockImplementation((command, args) => command === 'list_mcp_servers' ? Promise.resolve([
      { status: { connected: true }, tools: [{ server_id: 'server', name: 'lookup', description: 'Lookup', input_schema_json: '{}' }] },
      { status: { connected: false }, tools: [{ server_id: 'offline', name: 'lookup', description: 'Unavailable', input_schema_json: '{}' }] }
    ]) : base(command, args));
    mock.responses = [JSON.stringify({ type: 'tool', name: 'mcp__server__lookup', args: { arguments: '{"query":"test"}' } }), final];
    expect(await createWorkflowExecutor('D:/project', false).ai('model', 'task', new AbortController().signal)).toBe('answer');
    expect(mock.invoke).toHaveBeenCalledWith('create_agent_tool_call', expect.objectContaining({ draft: expect.objectContaining({ name: 'mcp__server__lookup', args_json: JSON.stringify({ arguments: '{"query":"test"}' }) }) }));
    const system = mock.invoke.mock.calls.find(c => c[0] === 'chat_stream')![1].request.messages[0].content;
    expect(system).not.toContain('mcp__offline__lookup');
  });
  it('interrupts an in-flight model request', async () => {
    mock.invoke.mockImplementation(async command => { if (command === 'chat_stream') await new Promise(() => {}); });
    const controller = new AbortController();
    const pending = workflowAI('model', 'task', controller.signal, 'role');
    await vi.waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('chat_stream', expect.anything()));
    controller.abort(); await expect(pending).rejects.toThrow('停止');
    expect(mock.invoke).toHaveBeenCalledWith('interrupt_chat_stream', expect.anything());
  });
  it('keeps AI conditions free of tool protocols', async () => {
    expect(await createWorkflowExecutor('D:/project', true).ai('model', 'condition', new AbortController().signal, '', false)).toBe('answer');
    expect(mock.invoke.mock.calls.some(c => c[0] === 'list_agent_tool_definitions')).toBe(false);
  });
  it('queues approvals for parallel AI nodes', async () => {
    mock.responses = [tool, tool, final, final];
    let release: ((v: boolean) => void) | undefined;
    mock.confirm.mockImplementationOnce(() => new Promise<boolean>(r => { release = r; }));
    const ex = createWorkflowExecutor('D:/project', false), signal = new AbortController().signal;
    const a = ex.ai('model', 'a', signal), b = ex.ai('model', 'b', signal);
    await vi.waitFor(() => expect(mock.confirm).toHaveBeenCalledTimes(1));
    release!(true); await Promise.all([a, b]); expect(mock.confirm).toHaveBeenCalledTimes(2);
  });
});
