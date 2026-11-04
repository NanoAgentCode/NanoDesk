import { describe, expect, it } from 'vitest';
import { interpolateArguments, runWorkflow, validateWorkflow, type Workflow, type WorkflowExecutor, type WorkflowNode } from './workflow';
const node = (id: string, kind: WorkflowNode['kind'] = 'ai'): WorkflowNode => ({ id, title: id, kind, x: 0, y: 0, prompt: id, modelId: 'model', tool: 'test', args: '{}', conditionMode: 'rule', operator: 'equals', operand: 'yes' });
const graph = (nodes: WorkflowNode[], pairs: [string, string, ('true' | 'false')?][]): Workflow => ({ id: 'w', name: 'test', projectPath: '', nodes, edges: pairs.map(([source, target, branch], i) => ({ id: String(i), source, target, branch })) });
const executor: WorkflowExecutor = { ai: async (_, p) => p, tool: async () => 'tool output' };
const run = (w: Workflow, ex = executor) => runWorkflow(w, ex, new AbortController().signal, () => {});
describe('workflow execution', () => {
  it('executes AI to parallel AI/tool to condition and merge as one workflow', async () => {
    const w = graph([
      node('input'), { ...node('ai'), prompt: '{{input.output}} analysis' },
      { ...node('tool', 'tool'), args: '{"input":"{{input.output}}"}' },
      { ...node('condition', 'condition'), prompt: '{{tool.output}}', operand: 'tool output' },
      { ...node('chosen'), prompt: '{{ai.output}} final' }, node('other'), node('merge', 'merge')
    ], [['input', 'ai'], ['input', 'tool'], ['tool', 'condition'], ['condition', 'chosen', 'true'], ['ai', 'chosen'], ['condition', 'other', 'false'], ['chosen', 'merge'], ['other', 'merge']]);
    const result = await run(w);
    expect(result.merge.output).toBe('input analysis final');
    expect(result.other.status).toBe('skipped'); expect(result.tool.status).toBe('success');
  });
  it('interpolates tool JSON strings safely', () => {
    expect(interpolateArguments('{"text":"{{a.output}}"}', { a: { status: 'success', output: '"quoted"\nline' } }, new Set(['a']))).toEqual({ text: '"quoted"\nline' });
  });
  it('invalidates downstream results after manually rerunning an upstream node', async () => {
    const result = await runWorkflow(graph([node('a'), node('b')], [['a', 'b']]), executor, new AbortController().signal, () => {}, { nodeId: 'a', previous: { b: { status: 'success', output: 'stale' } } });
    expect(result.b.status).toBe('pending');
  });
  it('rejects cycles and dangling edges', () => {
    expect(() => validateWorkflow(graph([node('a'), node('b')], [['a', 'b'], ['b', 'a']]))).toThrow('循环');
    expect(() => validateWorkflow(graph([node('a')], [['a', 'b']]))).toThrow('不存在');
  });
  it('runs independent branches concurrently then merges', async () => {
    let active = 0, peak = 0;
    const ex: WorkflowExecutor = { ...executor, ai: async (_, p) => { active++; peak = Math.max(peak, active); await new Promise(r => setTimeout(r, 5)); active--; return p; } };
    const result = await run(graph([node('a'), node('b'), node('m', 'merge')], [['a', 'm'], ['b', 'm']]), ex);
    expect(peak).toBe(2); expect(result.m.output).toBe('a\n\nb');
  });
  it('skips unselected descendants and merges only the chosen path', async () => {
    const c = { ...node('c', 'condition'), prompt: 'yes' };
    const result = await run(graph([c, node('a'), node('b'), node('d'), node('m', 'merge')], [['c', 'a', 'true'], ['c', 'b', 'false'], ['b', 'd'], ['a', 'm'], ['d', 'm']]));
    expect(result.b.status).toBe('skipped'); expect(result.d.status).toBe('skipped'); expect(result.m.output).toBe('a');
  });
  it('does not activate an unselected conditional node through another data dependency', async () => {
    const result = await run(graph([{ ...node('c', 'condition'), prompt: 'no' }, node('a'), node('b')], [['c', 'b', 'true'], ['a', 'b']]));
    expect(result.b.status).toBe('skipped');
  });
  it('isolates failure and retries without rerunning successful independent tools', async () => {
    let fail = true, tools = 0;
    const ex: WorkflowExecutor = { ai: async () => { if (fail) throw new Error('offline'); return 'ok'; }, tool: async () => { tools++; return 'done'; } };
    const w = graph([node('a'), node('b', 'tool'), node('m', 'merge')], [['a', 'm'], ['b', 'm']]);
    const previous = await run(w, ex); expect(previous.m.status).toBe('blocked'); expect(previous.b.status).toBe('success');
    fail = false;
    const result = await runWorkflow(w, ex, new AbortController().signal, () => {}, { previous, retryId: 'a' });
    expect(result.m.status).toBe('success'); expect(tools).toBe(1);
  });
  it('requires upstream results for a manual run and resolves variables', async () => {
    const w = graph([node('a'), { ...node('b'), prompt: '{{a.output}}!' }], [['a', 'b']]);
    await expect(runWorkflow(w, executor, new AbortController().signal, () => {}, { nodeId: 'b' })).rejects.toThrow('先运行');
    const result = await runWorkflow(w, executor, new AbortController().signal, () => {}, { nodeId: 'b', previous: { a: { status: 'success', output: 'hello' } } });
    expect(result.b.output).toBe('hello!');
  });
  it('allows manually running a merge after the unselected branch was skipped', async () => {
    const w = graph([node('a'), node('b'), node('m', 'merge')], [['a', 'm'], ['b', 'm']]);
    const result = await runWorkflow(w, executor, new AbortController().signal, () => {}, { nodeId: 'm', previous: { a: { status: 'success', output: 'selected' }, b: { status: 'skipped' } } });
    expect(result.m.output).toBe('selected');
  });
  it('selects the false AI branch and validates numeric conditions', async () => {
    const w = graph([{ ...node('c', 'condition'), conditionMode: 'ai' }, node('yes'), node('no')], [['c', 'yes', 'true'], ['c', 'no', 'false']]);
    const result = await run(w, { ...executor, ai: async () => 'false' });
    expect(result.yes.status).toBe('skipped'); expect(result.no.status).toBe('success');
    const invalid = await run(graph([{ ...node('numeric', 'condition'), prompt: 'invalid', operator: 'gt' }], []));
    expect(invalid.numeric.status).toBe('error');
  });
  it('rejects ambiguous AI conditions and references outside upstream', async () => {
    const result = await run(graph([{ ...node('c', 'condition'), conditionMode: 'ai' }, { ...node('a'), prompt: '{{c.output}}' }], []));
    expect(result.c.status).toBe('error'); expect(result.a.status).toBe('error');
  });
  it('stops scheduling downstream after cancellation', async () => {
    const controller = new AbortController();
    const result = await runWorkflow(graph([node('a'), node('b')], [['a', 'b']]), { ...executor, ai: async () => { controller.abort(); return 'late'; } }, controller.signal, () => {});
    expect(result.a.status).toBe('cancelled'); expect(result.b.status).toBe('cancelled');
  });
});
