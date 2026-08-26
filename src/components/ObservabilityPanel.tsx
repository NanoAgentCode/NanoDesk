import { Activity, ChevronDown, ChevronRight, RotateCcw, Trash2, Loader2 } from "lucide-react";
import type { ObservabilitySpan } from "../types";
import ObservabilityDetailPanel from "./ObservabilityDetailPanel";
import IconTooltipButton from "./IconTooltipButton";
import {
  buildObservabilitySpanDetail,
  formatDuration,
  formatShortTime
} from "./observabilityFormatters";

export interface ObservabilityTraceGroup {
  traceId: string;
  spans: ObservabilitySpan[];
  errors: number;
  duration: number;
  startedAt: string;
  lastOperation: string;
}

interface ObservabilityPanelProps {
  traces: ObservabilityTraceGroup[];
  selectedTrace: ObservabilityTraceGroup | null;
  timelineItems: ObservabilitySpan[];
  expandedRows: string[];
  isTimelineCollapsed: boolean;
  isLoading: boolean;
  spanCount: number;
  onRefresh: () => void;
  onClear: () => void;
  onSelectTrace: (traceId: string) => void;
  onToggleTimeline: () => void;
  onToggleRow: (spanId: string) => void;
}

export default function ObservabilityPanel({
  traces,
  selectedTrace,
  timelineItems,
  expandedRows,
  isTimelineCollapsed,
  isLoading,
  spanCount,
  onRefresh,
  onClear,
  onSelectTrace,
  onToggleTimeline,
  onToggleRow
}: ObservabilityPanelProps) {
  return (
    <div className="settings-tab-content observability-tab-content">
      <div className="observability-header">
        <div>
          <h3>链路追踪</h3>
          <p className="description">查看最近的本地调用链路、耗时和错误状态。</p>
        </div>
        <div className="observability-actions">
          <IconTooltipButton label={isLoading ? "刷新中" : "刷新链路"} onClick={onRefresh} disabled={isLoading}>
            {isLoading ? <Loader2 size={18} className="svg-spin" /> : <RotateCcw size={18} />}
          </IconTooltipButton>
          <IconTooltipButton label="清空链路" tone="danger" onClick={onClear} disabled={spanCount === 0}>
            <Trash2 size={18} />
          </IconTooltipButton>
        </div>
      </div>

      <div className="observability-grid">
        <aside className="observability-trace-list">
          {traces.map((trace) => (
            <button
              key={trace.traceId}
              className={selectedTrace?.traceId === trace.traceId ? "trace-config-row active" : "trace-config-row"}
              onClick={() => onSelectTrace(trace.traceId)}
              type="button"
            >
              <div className="trace-config-row-header">
                <strong>{trace.lastOperation || "trace"}</strong>
                <span className={`trace-indicator-badge ${trace.errors > 0 ? "error" : "success"}`}>
                  {trace.errors > 0 ? "有错误" : "正常"}
                </span>
              </div>
              <span>{trace.traceId} · {trace.spans.length} spans · {trace.duration} ms</span>
            </button>
          ))}
          {traces.length === 0 && (
            <div className="empty">暂无链路记录</div>
          )}
        </aside>

        <section className="observability-span-list">
          {selectedTrace ? (
            <>
              <div className="observability-trace-summary clickable" onClick={onToggleTimeline}>
                <div>
                  <strong>{selectedTrace.lastOperation || "chat_stream"}</strong>
                  <span>{selectedTrace.traceId}</span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                  <span>{selectedTrace.spans.length} 条消息 · {formatDuration(selectedTrace.duration)}</span>
                  {isTimelineCollapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
                </div>
              </div>
              {!isTimelineCollapsed && (
                <div className="observability-timeline">
                  {timelineItems.map((span, index) => {
                    const rowId = `span-${span.id}`;
                    const isExpanded = expandedRows.includes(rowId);
                    const detail = buildObservabilitySpanDetail(span);

                    return (
                      <div key={span.id} className={`observability-span-row ${span.status}`}>
                        <div className="observability-timeline-marker">
                          <span className="observability-status-dot" />
                          {index < timelineItems.length - 1 && <span className="observability-timeline-line" />}
                        </div>
                        <div className="observability-span-content">
                          <button className="timeline-row-toggle" onClick={() => onToggleRow(rowId)} type="button">
                            <span className="timeline-row-copy">
                              <strong>{span.operation}</strong>
                              <small>{span.category}{span.entity_type ? ` / ${span.entity_type}` : ""}</small>
                            </span>
                            <span className="observability-span-meta">
                              <span>{formatDuration(span.duration_ms ?? 0)}</span>
                              <span>{formatShortTime(span.started_at)}</span>
                            </span>
                            {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                          </button>
                          {isExpanded && detail && (
                            <div className="timeline-row-detail">
                              <ObservabilityDetailPanel detail={detail} />
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                  {timelineItems.length === 0 && (
                    <div className="empty">该链路暂无消息</div>
                  )}
                </div>
              )}
            </>
          ) : (
            <div className="archive-preview-placeholder">
              <Activity size={48} className="placeholder-icon" />
              <p>暂无可查看的链路</p>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
