import type { DocumentAnalysisProgress as AnalysisProgress } from "../../lib/backgroundAgent";

export default function DocumentAnalysisProgress({ progress }: { progress?: AnalysisProgress | null }) {
  if (!progress || progress.total <= 0) return null;
  const completed = Math.min(progress.total, Math.max(0, progress.completed));
  const label = progress.phase === "summary" ? "文档概要" : progress.phase === "reduce" ? `摘要归并 · 第 ${progress.level} 层` : "全文分析";
  return <section className="document-analysis-progress" role="status" aria-live="polite">
    <div><strong>{label}</strong><span>已完成 {completed}/{progress.total} 组{progress.reused > 0 ? ` · 复用 ${progress.reused} 组` : ""}</span></div>
    {progress.file_name && <small title={progress.file_name}>{progress.file_name}</small>}
    {progress.detail && <small>{progress.detail}{progress.requests ? ` · 已请求 ${progress.requests} 次` : ""}</small>}
    <progress aria-label={`${label}分组进度`} max={progress.total} value={completed} />
  </section>;
}
