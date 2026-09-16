import { useCallback, useEffect, useMemo, useState } from "react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import {
  createDefaultRoutingAssignments,
  isRoutingStrategy,
  normalizeRoutingAssignments,
  routeModel,
  type ModelRoutingDecision,
  type RoutingModelAssignments,
  type RoutingStrategy
} from "../lib/modelRouting";
import type { ModelConfig } from "../types";

const ROUTING_ENABLED_KEY = `${APP_STORAGE_PREFIX}-smart-routing-enabled`;
const ROUTING_STRATEGY_KEY = `${APP_STORAGE_PREFIX}-smart-routing-strategy`;
const ROUTING_ASSIGNMENTS_KEY = `${APP_STORAGE_PREFIX}-smart-routing-assignments`;

export interface UseModelRoutingReturn {
  enabled: boolean;
  strategy: RoutingStrategy;
  mode: "manual" | RoutingStrategy;
  assignments: RoutingModelAssignments;
  isStrategyAvailable: (strategy: RoutingStrategy) => boolean;
  setMode: (mode: string | null) => void;
  setStrategyModels: (strategy: RoutingStrategy, modelIds: string[]) => void;
  resolve: (content: string, hasImages?: boolean) => ModelRoutingDecision | null;
}

function readStoredAssignments(): Partial<Record<RoutingStrategy, string[]>> | null {
  const stored = localStorage.getItem(ROUTING_ASSIGNMENTS_KEY);
  if (!stored) return null;
  try {
    const parsed = JSON.parse(stored);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

export function useModelRouting(models: ModelConfig[], fallbackModelId: string): UseModelRoutingReturn {
  const [enabled, setEnabled] = useState(() => localStorage.getItem(ROUTING_ENABLED_KEY) === "true");
  const [strategy, setStrategy] = useState<RoutingStrategy>(() => {
    const stored = localStorage.getItem(ROUTING_STRATEGY_KEY);
    return isRoutingStrategy(stored) ? stored : "balanced";
  });
  const [storedAssignments, setStoredAssignments] = useState(readStoredAssignments);
  const assignments = useMemo(
    () => storedAssignments
      ? normalizeRoutingAssignments(storedAssignments, models)
      : createDefaultRoutingAssignments(models),
    [models, storedAssignments]
  );
  const isStrategyAvailable = useCallback(
    (value: RoutingStrategy) => assignments[value].length > 0,
    [assignments]
  );
  const effectiveEnabled = enabled && isStrategyAvailable(strategy);

  useEffect(() => {
    if (storedAssignments || models.filter((model) => model.id !== "embedding-config").length === 0) return;
    const migrated = createDefaultRoutingAssignments(models);
    setStoredAssignments(migrated);
    localStorage.setItem(ROUTING_ASSIGNMENTS_KEY, JSON.stringify(migrated));
  }, [models, storedAssignments]);

  const setMode = useCallback((mode: string | null) => {
    const nextEnabled = isRoutingStrategy(mode) && isStrategyAvailable(mode);
    setEnabled(nextEnabled);
    localStorage.setItem(ROUTING_ENABLED_KEY, String(nextEnabled));
    if (nextEnabled) {
      setStrategy(mode);
      localStorage.setItem(ROUTING_STRATEGY_KEY, mode);
    }
  }, [isStrategyAvailable]);

  const setStrategyModels = useCallback((value: RoutingStrategy, modelIds: string[]) => {
    setStoredAssignments((current) => {
      const base = current
        ? normalizeRoutingAssignments(current, models)
        : createDefaultRoutingAssignments(models);
      const next = normalizeRoutingAssignments({ ...base, [value]: modelIds }, models);
      localStorage.setItem(ROUTING_ASSIGNMENTS_KEY, JSON.stringify(next));
      return next;
    });
  }, [models]);

  const resolve = useCallback(
    (content: string, hasImages = false) => routeModel(
      models,
      content,
      strategy,
      fallbackModelId,
      hasImages,
      assignments[strategy]
    ),
    [assignments, fallbackModelId, models, strategy]
  );

  return {
    enabled: effectiveEnabled,
    strategy,
    mode: effectiveEnabled ? strategy : "manual",
    assignments,
    isStrategyAvailable,
    setMode,
    setStrategyModels,
    resolve
  };
}
