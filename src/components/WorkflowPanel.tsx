import { useEffect, useRef, useState } from 'react';
import { Button, Group, Select, TextInput, Textarea } from '@mantine/core';
import { GitBranch, Play, Plus, Square } from 'lucide-react';
import { listModelConfigs } from '../api/models';
import { isChatModel } from '../lib/modelCapabilities';
import { buildTaskPriceModelOptions } from '../lib/modelOptions';
import { createWorkflowExecutor } from '../lib/workflowRuntime';
import { runWorkflow, validateWorkflow, migrateWorkflow, workflowSignature, type Workflow, type WorkflowNode, type WorkflowRun } from '../lib/workflow';
import { APP_STORAGE_PREFIX } from '../config/brand';
import { confirmAction } from '../lib/dialogs';
import './WorkflowPanel.css';

const storageKey = `${APP_STORAGE_PREFIX}-workflows-v1`;
const labels = { ai: 'AI 提示词', condition: '条件', merge: '汇合' };
const statuses = { pending: '等待', running: '运行中', success: '成功', error: '失败', skipped: '跳过', blocked: '已阻断', cancelled: '已停止' };
interface Saved { workflows: Workflow[]; runs: WorkflowRun[] }
function readSaved(): Saved {
  const raw = localStorage.getItem(storageKey);
  if (!raw) return { workflows: [], runs: [] };
  const parsed = JSON.parse(raw) as Saved;
  if (!Array.isArray(parsed.workflows) || !Array.isArray(parsed.runs)) throw new Error('工作流存档格式无效');
  parsed.workflows = parsed.workflows.map(migrateWorkflow);
  parsed.workflows.forEach(validateWorkflow);
  for (const run of parsed.runs) for (const result of Object.values(run.results)) {
    if (result.status === 'running' || result.status === 'pending') { result.status = 'cancelled'; result.error = '应用或工作流页面已关闭，执行已中断'; }
  }
  return parsed;
}

export default function WorkflowPanel({ setNotice }: { setNotice: (message: string) => void }) {
  const [initial] = useState(() => { try { return { data: readSaved(), error: '' }; } catch (e) { return { data: { workflows: [], runs: [] } as Saved, error: String(e) }; } });
  const [saved, setSaved] = useState(initial.data);
  const [selected, setSelected] = useState(initial.data.workflows[0]?.id ?? '');
  const [nodeId, setNodeId] = useState('');
  const [models, setModels] = useState<{ value: string; label: string }[]>([]);
  const [source, setSource] = useState(''); const [target, setTarget] = useState('');
  const [branch, setBranch] = useState<string | null>('true');
  const [scale, setScale] = useState(1);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [dropPreview, setDropPreview] = useState<{ kind: WorkflowNode['kind']; x: number; y: number } | null>(null);
  const paletteDrag = useRef<WorkflowNode['kind'] | null>(null);
  const [connection, setConnection] = useState<{ source: string; x: number; y: number } | null>(null);
  const connectionSource = useRef<string | null>(null);
  const [busy, setBusy] = useState(false); const running = useRef(false);
  const [current, setCurrent] = useState<WorkflowRun | null>(null);
  const [historyId, setHistoryId] = useState('');
  const executionDetailRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (historyId) executionDetailRef.current?.scrollIntoView({ block: 'nearest' });
  }, [historyId]);
  const controller = useRef<AbortController | null>(null);
  const [allowCommand, setAllowCommand] = useState(false);
  const drag = useRef<{ id: string; x: number; y: number; startX: number; startY: number } | null>(null);
  const workflow = saved.workflows.find(w => w.id === selected);
  const defaultModel = workflow?.defaultModelId || models[0]?.value || '';
  const node = workflow?.nodes.find(n => n.id === nodeId);
  const visibleRun = historyId ? saved.runs.find(r => r.id === historyId) : current?.workflowId === selected ? current : saved.runs.find(r => r.workflowId === selected);
  useEffect(() => { connectionSource.current = null; setConnection(null); }, [selected, busy]);
  useEffect(() => {
    void listModelConfigs().then(m => {
      setModels(buildTaskPriceModelOptions(m.filter(isChatModel)));
    }).catch(e => setNotice(String(e)));
    return () => controller.current?.abort();
  }, [setNotice]);
  useEffect(() => {
    if (initial.error) return;
    try { localStorage.setItem(storageKey, JSON.stringify(saved)); }
    catch (e) { setNotice(`工作流保存失败：${String(e)}`); }
  }, [saved, initial.error, setNotice]);
  const edit = (update: (w: Workflow) => Workflow) => {
    if (running.current) return;
    setCurrent(null); setHistoryId('');
    setSaved(s => ({ ...s, workflows: s.workflows.map(w => w.id === selected ? update(w) : w) }));
  };
  const patchNode = (patch: Partial<WorkflowNode>) => edit(w => ({ ...w, nodes: w.nodes.map(n => n.id === nodeId ? { ...n, ...patch } : n) }));
  function addNode(kind: WorkflowNode['kind'], position?: { x: number; y: number }) {
    if (!workflow || running.current || initial.error) return;
    const id = `n_${crypto.randomUUID().slice(0, 8)}`;
    edit(w => ({ ...w, nodes: [...w.nodes, { id, title: labels[kind], kind, x: position?.x ?? 50 + (w.nodes.length % 3) * 290, y: position?.y ?? 60 + Math.floor(w.nodes.length / 3) * 200, prompt: '', systemPrompt: '', modelId: '', conditionMode: 'rule', operator: 'contains', operand: '' }] }));
    setNodeId(id);
  }
  function dropPosition(clientX: number, clientY: number) {
    const viewport = viewportRef.current!;
    const bounds = viewport.getBoundingClientRect();
    return { x: Math.max(0, Math.min(2150, (clientX - bounds.left + viewport.scrollLeft) / scale)), y: Math.max(0, Math.min(1430, (clientY - bounds.top + viewport.scrollTop) / scale)) };
  }
  async function execute(mode: 'all' | 'node' | 'retry', id?: string) {
    if (!workflow || running.current) return;
    const snapshot = structuredClone(workflow);
    const last = current?.workflowId === selected ? current : saved.runs.find(r => r.workflowId === selected);
    const previous = last?.snapshot && workflowSignature(last.snapshot) === workflowSignature(snapshot) ? last.results : undefined;
    const run: WorkflowRun = { id: crypto.randomUUID(), workflowId: selected, snapshot, startedAt: new Date().toISOString(), results: {} };
    const abort = new AbortController(); controller.current = abort; running.current = true; setBusy(true); setHistoryId('');
    const base = createWorkflowExecutor(snapshot.projectPath, allowCommand);
    const executor = { ...base, ai: (model: string, prompt: string, signal: AbortSignal, systemPrompt?: string, useTools?: boolean) => base.ai(model || defaultModel, prompt, signal, systemPrompt, useTools) };
    try {
      await runWorkflow(snapshot, executor, abort.signal, results => {
        run.results = results; setCurrent({ ...run });
        setSaved(s => ({ ...s, runs: [{ ...run }, ...s.runs.filter(r => r.id !== run.id)] }));
      }, mode === 'all' ? {} : { previous, ...(mode === 'node' ? { nodeId: id } : { retryId: id }) });
    } catch (e) { setNotice(String(e)); }
    finally { running.current = false; setBusy(false); controller.current = null; }
  }
  function connect(from = source, to = target) {
    if (!workflow || !from || !to || running.current || initial.error) return;
    const edge = { id: crypto.randomUUID(), source: from, target: to, branch: workflow.nodes.find(n => n.id === from)?.kind === 'condition' ? branch as 'true' | 'false' : undefined };
    if (workflow.edges.some(e => e.source === from && e.target === to && e.branch === edge.branch)) { setNotice('连线已存在'); return; }
    const updated = { ...workflow, edges: [...workflow.edges, edge] };
    try { validateWorkflow(updated); edit(() => updated); } catch (e) { setNotice(String(e)); }
  }
  const options = workflow?.nodes.map(n => ({ value: n.id, label: `${n.title} · ${n.id}` })) ?? [];
  return <section className="workflow-panel">
    <header className="workflow-header"><div><GitBranch size={25}/><div><h2>工作流画布</h2><p>连接 AI 节点与条件分支，AI 按需调用工具</p></div></div><Button disabled={busy || !!initial.error} leftSection={<Plus size={16}/>} onClick={() => {
      const w: Workflow = { id: crypto.randomUUID(), name: '新工作流', projectPath: '', nodes: [], edges: [] };
      setSaved(s => ({ ...s, workflows: [...s.workflows, w] })); setSelected(w.id); setNodeId(''); setCurrent(null); setHistoryId('');
    }}>新建工作流</Button></header>
    {initial.error && <p role="alert">无法读取存档，已停止写入以保护原数据：{initial.error}</p>}
    <div className="workflow-toolbar"><Select aria-label="选择工作流" placeholder="选择工作流" data={saved.workflows.map(w => ({ value: w.id, label: w.name }))} value={selected || null} disabled={busy} onChange={v => { setSelected(v ?? ''); setNodeId(''); setCurrent(null); setHistoryId(''); }}/>
      {workflow && <><TextInput aria-label="工作流名称" value={workflow.name} disabled={busy} onChange={e => { const name = e.currentTarget.value; edit(w => ({ ...w, name })); }}/><Select aria-label="默认模型" placeholder="默认模型" data={models} value={defaultModel || null} disabled={busy} onChange={v => edit(w => ({ ...w, defaultModelId: v ?? '' }))}/><Button disabled={busy || !workflow.nodes.length} leftSection={<Play size={14}/>} onClick={() => void execute('all')}>运行全部</Button><Button variant="light" color="red" disabled={!busy} leftSection={<Square size={14}/>} onClick={() => controller.current?.abort()}>停止</Button><Button variant="subtle" color="red" disabled={busy} onClick={async () => { if (await confirmAction(`删除工作流“${workflow.name}”及其运行记录？`)) { setSaved(s => ({ workflows: s.workflows.filter(w => w.id !== selected), runs: s.runs.filter(r => r.workflowId !== selected) })); setSelected(''); setNodeId(''); } }}>删除</Button></>}
    </div>
    {!workflow ? <div className="workflow-empty">创建一个工作流，添加 AI 节点开始。</div> : <>
      <div className="workflow-toolbar"><TextInput aria-label="工具项目目录" placeholder="AI 工具工作目录（留空仅生成文本）" value={workflow.projectPath} disabled={busy} onChange={e => { const projectPath = e.currentTarget.value; edit(w => ({ ...w, projectPath })); }}/><label><input type="checkbox" checked={allowCommand} disabled={busy} onChange={e => setAllowCommand(e.target.checked)}/>允许命令工具（仍需权限审批）</label><span className="workflow-hint">停止会取消模型生成并阻止后续调度；正在执行的工具可能完成当前操作。</span></div>
      <div className="workflow-toolbar">{(Object.keys(labels) as WorkflowNode['kind'][]).map(kind => <Button key={kind} variant="light" size="xs" disabled={busy || !!initial.error} draggable={!busy && !initial.error} title="拖到画布创建节点，也可点击添加" onDragStart={e => { if (running.current) { e.preventDefault(); return; } paletteDrag.current = kind; e.dataTransfer.setData('application/x-nanodesk-node', kind); e.dataTransfer.effectAllowed = 'copy'; }} onDragEnd={() => { paletteDrag.current = null; setDropPreview(null); }} onClick={() => addNode(kind)}>＋ {labels[kind]}</Button>)}<Select aria-label="连线起点" placeholder="起点" data={options} value={source || null} onChange={v => setSource(v ?? '')} disabled={busy}/><Select aria-label="条件连线分支" data={[{ value: 'true', label: '条件为真' }, { value: 'false', label: '条件为假' }]} value={branch} onChange={setBranch} disabled={busy}/><Select aria-label="连线终点" placeholder="终点" data={options} value={target || null} onChange={v => setTarget(v ?? '')} disabled={busy}/><Button size="xs" disabled={busy || !source || !target} onClick={() => connect()}>连接</Button><Button size="xs" variant="subtle" onClick={() => setScale(s => Math.max(.4, s - .1))}>−</Button><span>{Math.round(scale * 100)}%</span><Button size="xs" variant="subtle" onClick={() => setScale(s => Math.min(1.8, s + .1))}>＋</Button></div>
      <div className="workflow-body"><div ref={viewportRef} className={`workflow-viewport ${dropPreview ? 'drop-active' : ''}`} onDragOver={e => { const kind = paletteDrag.current; if (!kind || running.current) return; e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; setDropPreview({ kind, ...dropPosition(e.clientX, e.clientY) }); }} onDragLeave={e => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropPreview(null); }} onDrop={e => { const kind = paletteDrag.current; if (!kind || running.current) return; e.preventDefault(); addNode(kind, dropPosition(e.clientX, e.clientY)); paletteDrag.current = null; setDropPreview(null); }}><div className="workflow-world" style={{ width: 2400 * scale, height: 1600 * scale }}><div style={{ width: 2400, height: 1600, transform: `scale(${scale})`, transformOrigin: '0 0' }}>
        {dropPreview && <div className="workflow-drop-preview" style={{ left: dropPreview.x, top: dropPreview.y }}>＋ {labels[dropPreview.kind]}</div>}
        <svg className="workflow-edges" width="2400" height="1600">{connection && (() => { const a = workflow.nodes.find(n => n.id === connection.source); return a ? <path className="workflow-connection-preview" d={`M ${a.x + 240} ${a.y + 60} C ${a.x + 290} ${a.y + 60}, ${connection.x - 50} ${connection.y}, ${connection.x} ${connection.y}`} /> : null; })()}{workflow.edges.map(e => { const a = workflow.nodes.find(n => n.id === e.source)!, b = workflow.nodes.find(n => n.id === e.target)!; return <g key={e.id}><path d={`M ${a.x + 240} ${a.y + 60} C ${a.x + 290} ${a.y + 60}, ${b.x - 50} ${b.y + 60}, ${b.x} ${b.y + 60}`} /><text x={(a.x + 240 + b.x) / 2} y={(a.y + b.y) / 2 + 48}>{e.branch === 'true' ? '真 →' : e.branch === 'false' ? '假 →' : '→'}</text></g>; })}</svg>
        {workflow.nodes.map(n => { const result = visibleRun?.results[n.id]; return <article key={n.id} className={`workflow-node ${nodeId === n.id ? 'selected' : ''} ${result?.status ?? ''}`} style={{ left: n.x, top: n.y }} onClick={() => setNodeId(n.id)}>
          <button type="button" className="workflow-port workflow-port-input" data-input-node={n.id} aria-label={`${n.title} 输入端点`} title="输入：将上游输出端点拖到这里" disabled={busy} onClick={e => e.stopPropagation()} />
          <button type="button" className="workflow-port workflow-port-output" aria-label={`${n.title} 输出端点`} title={n.kind === 'condition' ? `输出：拖到下游输入端点（当前分支：${branch === 'true' ? '真' : '假'}）` : '输出：拖到下游输入端点'} disabled={busy} onClick={e => e.stopPropagation()}
            onPointerDown={e => { if (running.current || e.button !== 0) return; e.stopPropagation(); e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); connectionSource.current = n.id; setConnection({ source: n.id, x: n.x + 240, y: n.y + 60 }); }}
            onPointerMove={e => { if (connectionSource.current !== n.id) return; const viewport = viewportRef.current!; const bounds = viewport.getBoundingClientRect(); setConnection({ source: n.id, x: (e.clientX - bounds.left + viewport.scrollLeft) / scale, y: (e.clientY - bounds.top + viewport.scrollTop) / scale }); }}
            onPointerUp={e => { if (connectionSource.current !== n.id) return; e.stopPropagation(); const input = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLButtonElement>('[data-input-node]'); if (input && !input.disabled) connect(n.id, input.dataset.inputNode); connectionSource.current = null; setConnection(null); }}
            onPointerCancel={() => { connectionSource.current = null; setConnection(null); }}
            onLostPointerCapture={() => { connectionSource.current = null; setConnection(null); }}
            onKeyDown={e => { if (e.key === 'Escape') { connectionSource.current = null; setConnection(null); } }} />
          <div className="workflow-node-heading" onPointerDown={e => { if (busy) return; e.currentTarget.setPointerCapture(e.pointerId); drag.current = { id: n.id, x: n.x, y: n.y, startX: e.clientX, startY: e.clientY }; }} onPointerMove={e => { const d = drag.current; if (!d || d.id !== n.id) return; const x = Math.max(0, Math.min(2150, d.x + (e.clientX - d.startX) / scale)), y = Math.max(0, Math.min(1430, d.y + (e.clientY - d.startY) / scale)); setSaved(s => ({ ...s, workflows: s.workflows.map(w => w.id === selected ? { ...w, nodes: w.nodes.map(v => v.id === n.id ? { ...v, x, y } : v) } : w) })); }} onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }}><small>{labels[n.kind]}</small><strong>{n.title}</strong></div>
          <p>{result ? statuses[result.status] : '未运行'}</p><Group gap="xs"><Button size="compact-xs" variant="light" disabled={busy} onClick={e => { e.stopPropagation(); void execute('node', n.id); }}>运行节点</Button>{result?.status === 'error' && <Button size="compact-xs" disabled={busy} onClick={() => void execute('retry', n.id)}>重试并继续</Button>}</Group>
        </article>; })}
      </div></div></div>
      <aside className="workflow-inspector">{node ? <>
        <h3>节点设置</h3><code>{node.id}</code><TextInput label="名称" value={node.title} disabled={busy} onChange={e => patchNode({ title: e.currentTarget.value })}/>
        {(node.kind === 'ai' || node.kind === 'condition' && node.conditionMode === 'ai') && <Select label="模型" placeholder="使用默认模型" clearable data={models} value={node.modelId || null} disabled={busy} onChange={v => patchNode({ modelId: v ?? '' })}/>}
        {node.kind === 'condition' && <><Select label="判断方式" data={[{ value: 'rule', label: '固定规则' }, { value: 'ai', label: 'AI 语义判断' }]} value={node.conditionMode} disabled={busy} onChange={v => patchNode({ conditionMode: v as 'rule' | 'ai' })}/>{node.conditionMode === 'rule' && <><Select label="比较规则" data={[{ value: 'contains', label: '包含' }, { value: 'equals', label: '等于' }, { value: 'gt', label: '数值大于' }]} value={node.operator} disabled={busy} onChange={v => patchNode({ operator: v as WorkflowNode['operator'] })}/><TextInput label="比较值" value={node.operand} disabled={busy} onChange={e => patchNode({ operand: e.currentTarget.value })}/></>}</>}
        {node.kind === 'ai' && <Textarea label="系统提示词" description="定义角色、行为和输出要求" autosize minRows={3} value={node.systemPrompt ?? ''} disabled={busy} onChange={e => patchNode({ systemPrompt: e.currentTarget.value })}/>}
        {node.kind !== 'merge' && <Textarea label={node.kind === 'condition' ? '判断内容 / 指令' : '用户提示词'} autosize minRows={5} value={node.prompt} disabled={busy} onChange={e => patchNode({ prompt: e.currentTarget.value })}/>}
        <p className="workflow-hint">系统提示词和用户提示词均支持引用上游：{'{{节点ID.output}}'}。设置项目目录后，AI 自行判断是否调用已接入的工具，沿用现有权限审批；留空则仅生成文本。</p>
        <Button variant="subtle" color="red" disabled={busy} onClick={() => { edit(w => ({ ...w, nodes: w.nodes.filter(n => n.id !== nodeId), edges: w.edges.filter(e => e.source !== nodeId && e.target !== nodeId) })); setNodeId(''); }}>删除节点</Button>
        {visibleRun?.results[node.id] && <section className="workflow-execution-detail" ref={executionDetailRef} aria-label="执行详情"><h3>执行详情</h3><p className="workflow-hint">{new Date(visibleRun.startedAt).toLocaleString()} · {statuses[visibleRun.results[node.id].status]}</p>{visibleRun.results[node.id].systemInput && <><h4>系统提示词</h4><pre>{visibleRun.results[node.id].systemInput}</pre></>}<h4>用户输入</h4><pre>{visibleRun.results[node.id].input || '（空输入）'}</pre><h4>输出</h4><pre>{visibleRun.results[node.id].output || '（暂无输出）'}</pre>{visibleRun.results[node.id].error && <pre role="alert">{visibleRun.results[node.id].error}</pre>}</section>}
      </> : <p className="workflow-hint">{workflow.nodes.length ? '从节点右侧输出端点拖到另一个节点左侧输入端点建立连线。条件分支沿用上方真 / 假选项；拖动节点标题调整位置。' : '将上方的「＋ AI 提示词」「＋ 条件」或「＋ 汇合」拖到画布创建节点，也可点击添加，再在这里编辑内容。'}</p>}
      <h3>连线</h3>{workflow.edges.map(e => <div className="workflow-edge-row" key={e.id}><span>{workflow.nodes.find(n => n.id === e.source)?.title} → {workflow.nodes.find(n => n.id === e.target)?.title}{e.branch ? ` (${e.branch})` : ''}</span><button aria-label={`删除连线 ${e.id}`} disabled={busy} onClick={() => edit(w => ({ ...w, edges: w.edges.filter(v => v.id !== e.id) }))}>×</button></div>)}
      <h3>运行记录</h3><Select aria-label="运行记录" clearable placeholder="最新运行" value={historyId || null} onChange={v => { setHistoryId(v ?? ''); const record = saved.runs.find(r => r.id === v); if (record && (!nodeId || !record.results[nodeId])) { const ids = Object.keys(record.results).filter(id => workflow.nodes.some(n => n.id === id)); setNodeId(ids.find(id => record.results[id].input !== undefined || record.results[id].output !== undefined) ?? ids[0] ?? ''); } }} data={saved.runs.filter(r => r.workflowId === selected).map(r => ({ value: r.id, label: new Date(r.startedAt).toLocaleString() }))}/>
      </aside></div>
    </>}
  </section>;
}
