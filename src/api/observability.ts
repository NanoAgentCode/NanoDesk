import { invoke } from "@tauri-apps/api/core";
import type { ObservabilitySpan, UsageAnalysis } from "../types";

export function listObservabilitySpans(limit = 200) {
  return invoke<ObservabilitySpan[]>("list_observability_spans", { limit });
}

export function clearObservabilitySpans() {
  return invoke<void>("clear_observability_spans");
}

export function getUsageAnalysis() {
  return invoke<UsageAnalysis>("get_usage_analysis");
}
