import { useEffect, useState } from "react";
import { Activity, BarChart3, Clock3, MessagesSquare, RefreshCw } from "lucide-react";
import { getUsageAnalysis } from "../../api";
import type { UsageAnalysis } from "../../types";
import IconTooltipButton from "../IconTooltipButton";

const EMPTY_ANALYSIS: UsageAnalysis = {
  conversation_count: 0,
  message_count: 0,
  model_usage: [],
  prompt_tokens: 0,
  completion_tokens: 0,
  total_tokens: 0,
  token_trend: [],
  latency_call_count: 0,
  average_latency_ms: 0,
  p95_latency_ms: 0
};

function formatNumber(value: number) {
  return new Intl.NumberFormat("zh-CN").format(Math.round(value));
}

export default function SettingsUsageTab() {
  const [analysis, setAnalysis] = useState(EMPTY_ANALYSIS);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    setLoading(true);
    setError("");
    try {
      setAnalysis(await getUsageAnalysis());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  const trend = analysis.token_trend.slice(-30);
  const maxTokens = Math.max(1, ...trend.map((point) => point.total_tokens));
  const modelLegend = Array.from(
    new Map(
      trend.flatMap((point) => point.models).map((model) => [
        model.model_config_id || "unassigned",
        model.model_name
      ])
    ).entries()
  ).map(([id, name], index) => ({
    id,
    name,
    color: `hsl(${Math.round(index * 137.508) % 360} 70% 60%)`
  }));
  const modelColors = new Map(modelLegend.map((model) => [model.id, model.color]));

  return (
    <div className="settings-tab-content usage-tab-content">
      <div className="usage-header">
        <div>
          <h3>用量分析</h3>
          <p className="description">了解本地会话、消息、模型回答、Token 估算与模型调用延迟。</p>
        </div>
        <IconTooltipButton label={loading ? "刷新中" : "刷新用量"} onClick={() => void refresh()} disabled={loading}>
          <RefreshCw size={18} className={loading ? "svg-spin" : ""} />
        </IconTooltipButton>
      </div>

      {error && <div className="usage-error">{error}</div>}

      <div className="usage-metric-grid">
        <div className="usage-metric"><MessagesSquare size={18} /><span>会话数</span><strong>{formatNumber(analysis.conversation_count)}</strong></div>
        <div className="usage-metric"><BarChart3 size={18} /><span>消息数</span><strong>{formatNumber(analysis.message_count)}</strong></div>
        <div className="usage-metric"><Activity size={18} /><span>平均延迟</span><strong>{formatNumber(analysis.average_latency_ms)} ms</strong><small>{formatNumber(analysis.latency_call_count)} 次调用</small></div>
        <div className="usage-metric"><Clock3 size={18} /><span>P95 延迟</span><strong>{formatNumber(analysis.p95_latency_ms)} ms</strong></div>
      </div>

      <section className="usage-card">
        <div className="usage-card-heading"><div><h4>Token 用量</h4><p>按已保存消息内容估算，不作为供应商账单依据。</p></div><strong>{formatNumber(analysis.total_tokens)}</strong></div>
        <div className="usage-token-breakdown">
          <span>Prompt <strong>{formatNumber(analysis.prompt_tokens)}</strong></span>
          <span>Completion <strong>{formatNumber(analysis.completion_tokens)}</strong></span>
        </div>
        <div className="usage-model-legend">
          {modelLegend.map((model) => <span key={model.id}><i style={{ background: model.color }} />{model.name}</span>)}
        </div>
        <div className="usage-trend-scroll">
          <div className="usage-trend" aria-label="最近 30 天 Token 趋势">
            {trend.map((point) => (
              <div className="usage-trend-column" key={point.date} title={`${point.date}: ${formatNumber(point.total_tokens)} Token`}>
                <div className="usage-trend-bar" style={{ height: `${Math.max(4, point.total_tokens / maxTokens * 100)}%` }}>
                  {point.models.map((model) => (
                    <i
                      key={model.model_config_id || "unassigned"}
                      style={{
                        background: modelColors.get(model.model_config_id || "unassigned"),
                        height: `${model.tokens / Math.max(1, point.total_tokens) * 100}%`
                      }}
                      title={`${model.model_name}: ${formatNumber(model.tokens)} Token`}
                    />
                  ))}
                </div>
                <span>{point.date.slice(5)}</span>
              </div>
            ))}
            {trend.length === 0 && <div className="empty">暂无 Token 数据</div>}
          </div>
        </div>
      </section>

      <section className="usage-card usage-models">
        <div className="usage-card-heading"><div><h4>模型使用次数</h4><p>以已保存的助手回答数统计。</p></div></div>
        {analysis.model_usage.map((item) => <div className="usage-model-row" key={item.model_config_id || "unassigned"}><span>{item.model_name}</span><strong>{formatNumber(item.count)} 次</strong></div>)}
        {analysis.model_usage.length === 0 && <div className="empty">暂无模型使用记录</div>}
      </section>

    </div>
  );
}
