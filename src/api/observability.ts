import { invoke } from "@tauri-apps/api/core";
import type { UsageAnalysis } from "../types";

export function getUsageAnalysis() {
  return invoke<UsageAnalysis>("get_usage_analysis");
}
