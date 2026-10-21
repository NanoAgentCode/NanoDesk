import { useState } from "react";
import { FileText, FolderOpen } from "lucide-react";
import { openProjectFileLocation } from "../api/files";
import type { Deliverable } from "../lib/deliverables";
import FriendlyError from "./FriendlyError";

export default function DeliverablesPanel({ files, projectPath }: { files: Deliverable[]; projectPath: string }) {
  const [error, setError] = useState("");
  return <div className="deliverables-panel">
    <p className="task-summary-loading">当前对话的文件 · {files.length} 个</p>
    {error && <FriendlyError error={error} />}
    {([['upload', '用户上传'], ['intermediate', '中间文件'], ['final', '最终交付']] as const).map(([kind, label]) => {
      const group = files.filter(file => file.kind === kind);
      return <section key={kind} aria-label={label} className="deliverables-group">
        <h3>{label}<small>{group.length}</small></h3>
        {!group.length ? <p className="task-summary-loading">暂无{label}文件</p> : group.map(file => <div className="deliverable-file" key={file.id}>
          <FileText size={17} aria-hidden="true" />
          <div><strong>{file.name}</strong><small>{file.path || "对话文档附件"}</small></div>
          {file.path && projectPath && <button type="button" title="在文件夹中显示" aria-label={`在文件夹中显示 ${file.name}`} onClick={() => {
            setError("");
            void openProjectFileLocation(projectPath, file.path!).catch(error => setError(String(error)));
          }}><FolderOpen size={16} /></button>}
        </div>)}
      </section>;
    })}
    <p className="task-summary-loading">文件按每轮任务目标与用途分类；最终交付物会保留，后续作为输入使用不会降级。</p>
  </div>;
}
