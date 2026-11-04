export type NodeKind = 'ai' | 'condition' | 'merge';
export interface WorkflowNode {
  id: string; title: string; kind: NodeKind; x: number; y: number;
  prompt: string; systemPrompt?: string; modelId: string;
  conditionMode: 'rule' | 'ai'; operator: 'contains' | 'equals' | 'gt'; operand: string;
}
export interface WorkflowEdge { id: string; source: string; target: string; branch?: 'true' | 'false' }
export interface Workflow { id: string; name: string; projectPath: string; defaultModelId?: string; nodes: WorkflowNode[]; edges: WorkflowEdge[] }
export type NodeStatus = 'pending' | 'running' | 'success' | 'error' | 'skipped' | 'blocked' | 'cancelled';
export interface NodeResult { status: NodeStatus; input?: string; systemInput?: string; output?: string; error?: string; branch?: boolean }
export interface WorkflowRun { id: string; workflowId: string; startedAt: string; snapshot?: Workflow; results: Record<string, NodeResult> }
export interface WorkflowExecutor {
  ai(modelId: string, prompt: string, signal: AbortSignal, systemPrompt?: string, useTools?: boolean): Promise<string>;
}
export function migrateWorkflow(w: Workflow): Workflow {
  return { ...w, nodes: w.nodes.map(node => {
    const legacy = node as WorkflowNode & { kind: string; tool?: string; args?: string };
    if ((legacy.kind as string) !== 'tool') return { ...node, systemPrompt: node.systemPrompt ?? '' };
    const { tool, args, ...rest } = legacy;
    return { ...rest, kind: 'ai' as const, systemPrompt: '', prompt: `请判断是否需要调用工具完成以下任务，并返回最终结果。\n原工具：${tool ?? ''}\n原参数：${args ?? '{}'}` };
  }) };
}
export function validateWorkflow(w: Workflow): void {
  const ids = new Set(w.nodes.map(n => n.id));
  if (ids.size !== w.nodes.length) throw new Error('节点 ID 重复');
  const visiting = new Set<string>(), done = new Set<string>();
  for (const e of w.edges) {
    if (!ids.has(e.source) || !ids.has(e.target)) throw new Error('连线节点不存在');
    if (w.nodes.find(n => n.id === e.source)?.kind === 'condition' && !e.branch) throw new Error('条件连线必须选择分支');
  }
  function visit(id: string) {
    if (visiting.has(id)) throw new Error('工作流不支持循环');
    if (done.has(id)) return;
    visiting.add(id);
    w.edges.filter(e => e.source === id).forEach(e => visit(e.target));
    visiting.delete(id); done.add(id);
  }
  ids.forEach(visit);
}
export function interpolate(text: string, results: Record<string, NodeResult>, allowed: Set<string>): string {
  return text.replace(/\{\{\s*([^{}\s]+)\.output\s*\}\}/g, (_, id: string) => {
    if (!allowed.has(id)) throw new Error(`只能引用上游节点：${id}`);
    if (results[id]?.status !== 'success') throw new Error(`上游节点 ${id} 尚无可用结果`);
    return results[id].output ?? '';
  });
}
export function upstream(w: Workflow, id: string): Set<string> {
  const found = new Set<string>();
  function add(target: string) { w.edges.filter(e => e.target === target).forEach(e => {
    if (!found.has(e.source)) { found.add(e.source); add(e.source); }
  }); }
  add(id); return found;
}
export function workflowSignature(w: Workflow): string {
  return JSON.stringify({ ...w, name: '', nodes: w.nodes.map(n => ({ ...n, x: 0, y: 0, title: '' })) });
}
export async function runWorkflow(
  w: Workflow, executor: WorkflowExecutor, signal: AbortSignal,
  onChange: (results: Record<string, NodeResult>) => void,
  options: { previous?: Record<string, NodeResult>; nodeId?: string; retryId?: string } = {}
): Promise<Record<string, NodeResult>> {
  validateWorkflow(w);
  const results: Record<string, NodeResult> = Object.fromEntries(w.nodes.map(n => [n.id, { status: 'pending' }]));
  if (options.previous) for (const n of w.nodes) if (options.previous[n.id]) results[n.id] = { ...options.previous[n.id] };
  if (options.retryId) {
    for (const n of w.nodes) if (n.id === options.retryId || upstream(w, n.id).has(options.retryId)) results[n.id] = { status: 'pending' };
  }
  if (options.nodeId) for (const n of w.nodes) if (upstream(w, n.id).has(options.nodeId)) results[n.id] = { status: 'pending' };
  const emit = () => onChange(Object.fromEntries(Object.entries(results).map(([id, r]) => [id, { ...r }])));
  const active = (e: WorkflowEdge) => !e.branch || results[e.source].branch === (e.branch === 'true');
  async function execute(n: WorkflowNode) {
    const r: NodeResult = { status: 'running' }; results[n.id] = r; emit();
    try {
      const allowed = upstream(w, n.id);
      r.input = interpolate(n.prompt, results, allowed);
      r.systemInput = interpolate(n.systemPrompt ?? '', results, allowed);
      if (signal.aborted) throw new Error('运行已停止');
      if (n.kind === 'merge') r.output = w.edges.filter(e => e.target === n.id && active(e) && results[e.source].status === 'success').map(e => results[e.source].output ?? '').join('\n\n');
      else if (n.kind === 'condition') {
        if (n.conditionMode === 'ai') {
          const answer = (await executor.ai(n.modelId, `${r.input}\n仅输出 true 或 false，禁止其他文字。`, signal, r.systemInput, false)).trim();
          if (answer !== 'true' && answer !== 'false') throw new Error('AI 条件必须返回 true 或 false');
          r.branch = answer === 'true';
        } else {
          const operand = interpolate(n.operand, results, allowed);
          if (n.operator === 'gt' && (!Number.isFinite(Number(r.input)) || !Number.isFinite(Number(operand)))) throw new Error('数值比较需要有效数字');
          r.branch = n.operator === 'contains' ? r.input.includes(operand) : n.operator === 'equals' ? r.input === operand : Number(r.input) > Number(operand);
        }
        r.output = String(r.branch);
      } else r.output = await executor.ai(n.modelId, r.input, signal, r.systemInput);
      r.status = signal.aborted ? 'cancelled' : 'success';
    } catch (e) { r.status = signal.aborted ? 'cancelled' : 'error'; r.error = String(e); }
    emit();
  }
  if (options.nodeId) {
    const n = w.nodes.find(n => n.id === options.nodeId);
    if (!n) throw new Error('节点不存在');
    for (const e of w.edges.filter(e => e.target === n.id)) {
      if (n.kind !== 'merge' && results[e.source].status === 'skipped') throw new Error('节点不在已选择的条件路径上');
      if (results[e.source].status !== 'success' && results[e.source].status !== 'skipped') throw new Error('请先运行上游节点');
      if (n.kind !== 'merge' && results[e.source].status === 'success' && !active(e)) throw new Error('节点不在已选择的条件路径上');
    }
    if (w.edges.some(e => e.target === n.id) && !w.edges.some(e => e.target === n.id && active(e) && results[e.source].status === 'success')) throw new Error('节点无活动的上游路径');
    await execute(n); return results;
  }
  while (w.nodes.some(n => results[n.id].status === 'pending')) {
    if (signal.aborted) { for (const r of Object.values(results)) if (r.status === 'pending') r.status = 'cancelled'; break; }
    const ready: WorkflowNode[] = [];
    let changed = false;
    for (const n of w.nodes.filter(n => results[n.id].status === 'pending')) {
      const incoming = w.edges.filter(e => e.target === n.id);
      if (incoming.some(e => ['pending', 'running'].includes(results[e.source].status))) continue;
      const gates = [...new Set(incoming.filter(e => e.branch).map(e => e.source))];
      if (n.kind !== 'merge' && (incoming.some(e => results[e.source].status === 'skipped') || gates.some(source => results[source].status === 'success' && !incoming.some(e => e.source === source && active(e))))) {
        results[n.id] = { status: 'skipped' }; changed = true; continue;
      }
      const selected = incoming.filter(e => results[e.source].status !== 'skipped' && active(e));
      if (selected.some(e => results[e.source].status !== 'success')) { results[n.id] = { status: 'blocked' }; changed = true; }
      else if (incoming.length && !selected.length) { results[n.id] = { status: 'skipped' }; changed = true; }
      else ready.push(n);
    }
    emit();
    if (ready.length) await Promise.all(ready.map(execute));
    else if (!changed) throw new Error('工作流存在无法调度的节点');
  }
  emit(); return results;
}
