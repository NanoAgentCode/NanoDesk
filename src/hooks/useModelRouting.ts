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
import { isChatModel } from "../lib/modelCapabilities";

const ROUTING_ENABLED_KEY = `${APP_STORAGE_PREFIX}-smart-routing-enabled`;
const ROUTING_STRATEGY_KEY = `${APP_STORAGE_PREFIX}-smart-routing-strategy`;
const ROUTING_ASSIGNMENTS_KEY = `${APP_STORAGE_PREFIX}-smart-routing-assignments`;
const FIXED_MODELS_KEY = `${APP_STORAGE_PREFIX}-fixed-models`;
const FALLBACK_MODEL_KEY = `${APP_STORAGE_PREFIX}-fallback-model`;

export interface UseModelRoutingReturn {
  enabled: boolean;
  strategy: RoutingStrategy;
  mode: "manual" | RoutingStrategy;
  assignments: RoutingModelAssignments;
  fixedModelIds: string[];
  fallbackModelId: string;
  isStrategyAvailable: (strategy: RoutingStrategy) => boolean;
  setMode: (mode: string | null) => void;
  setStrategyModel: (strategy: RoutingStrategy, modelId: string | null) => void;
  setFixedModels: (modelIds: string[]) => void;
  setFallbackModel: (modelId: string | null) => void;
  resolve: (content: string, hasImages?: boolean) => ModelRoutingDecision | null;
}

function readStoredModelIds(key: string): string[] | null {
  const stored = localStorage.getItem(key);
  if (!stored) return null;
  try {
    const parsed = JSON.parse(stored);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : null;
  } catch {
    return null;
  }
}

type StoredRoutingAssignments = Partial<Record<RoutingStrategy, string | string[] | null>>;

function readStoredAssignments(): StoredRoutingAssignments | null {
  const stored = localStorage.getItem(ROUTING_ASSIGNMENTS_KEY);
  if (!stored) return null;
  try {
    const parsed: unknown = JSON.parse(stored);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return Object.fromEntries(
      Object.entries(parsed).map(([key, value]) => [
        key,
        Array.isArray(value)
          ? value.filter((item): item is string => typeof item === "string")
          : typeof value === "string" ? value : null
      ])
    );
  } catch {
    return null;
  }
}

export function useModelRouting(models: ModelConfig[], currentModelId: string): UseModelRoutingReturn {
  const [enabled, setEnabled] = useState(() => localStorage.getItem(ROUTING_ENABLED_KEY) === "true");
  const [strategy, setStrategy] = useState<RoutingStrategy>(() => {
    const stored = localStorage.getItem(ROUTING_STRATEGY_KEY);
    return isRoutingStrategy(stored) ? stored : "balanced";
  });
  const [storedAssignments, setStoredAssignments] = useState<StoredRoutingAssignments | null>(readStoredAssignments);
  const [storedFixedModelIds, setStoredFixedModelIds] = useState(() => readStoredModelIds(FIXED_MODELS_KEY));
  const [storedFallbackModelId, setStoredFallbackModelId] = useState(() => localStorage.getItem(FALLBACK_MODEL_KEY) || "");
  const chatModelIds = useMemo(() => models.filter(isChatModel).map((model) => model.id), [models]);
  const fixedModelIds = useMemo(() => {
    const source = storedFixedModelIds ?? chatModelIds;
    const validIds = new Set(chatModelIds);
    return [...new Set(source)].filter((id) => validIds.has(id));
  }, [chatModelIds, storedFixedModelIds]);
  const fallbackModelId = chatModelIds.includes(storedFallbackModelId)
    ? storedFallbackModelId
    : fixedModelIds[0] || chatModelIds[0] || "";
  const assignments = useMemo(
    () => storedAssignments
      ? normalizeRoutingAssignments(storedAssignments, models)
      : createDefaultRoutingAssignments(models),
    [models, storedAssignments]
  );
  const isStrategyAvailable = useCallback(
    (value: RoutingStrategy) => Boolean(assignments[value]),
    [assignments]
  );
  const effectiveEnabled = enabled && isStrategyAvailable(strategy);

  useEffect(() => {
    if (storedAssignments || models.filter(isChatModel).length === 0) return;
    const migrated = createDefaultRoutingAssignments(models);
    setStoredAssignments(migrated);
    localStorage.setItem(ROUTING_ASSIGNMENTS_KEY, JSON.stringify(migrated));
  }, [models, storedAssignments]);

  useEffect(() => {
    if (storedFixedModelIds || chatModelIds.length === 0) return;
    setStoredFixedModelIds(chatModelIds);
    localStorage.setItem(FIXED_MODELS_KEY, JSON.stringify(chatModelIds));
  }, [chatModelIds, storedFixedModelIds]);

  useEffect(() => {
    if (!fallbackModelId || fallbackModelId === storedFallbackModelId) return;
    setStoredFallbackModelId(fallbackModelId);
    localStorage.setItem(FALLBACK_MODEL_KEY, fallbackModelId);
  }, [fallbackModelId, storedFallbackModelId]);

  const setMode = useCallback((mode: string | null) => {
    const nextEnabled = isRoutingStrategy(mode) && isStrategyAvailable(mode);
    setEnabled(nextEnabled);
    localStorage.setItem(ROUTING_ENABLED_KEY, String(nextEnabled));
    if (nextEnabled) {
      setStrategy(mode);
      localStorage.setItem(ROUTING_STRATEGY_KEY, mode);
    }
  }, [isStrategyAvailable]);

  const setStrategyModel = useCallback((value: RoutingStrategy, modelId: string | null) => {
    setStoredAssignments((current) => {
      const base = current ?? createDefaultRoutingAssignments(models);
      const next = normalizeRoutingAssignments({ ...base, [value]: modelId }, models);
      localStorage.setItem(ROUTING_ASSIGNMENTS_KEY, JSON.stringify(next));
      return next;
    });
  }, [models]);

  const setFixedModels = useCallback((modelIds: string[]) => {
    const validIds = new Set(models.filter(isChatModel).map((model) => model.id));
    const next = [...new Set(modelIds)].filter((id) => validIds.has(id));
    setStoredFixedModelIds(next);
    localStorage.setItem(FIXED_MODELS_KEY, JSON.stringify(next));
  }, [models]);

  const setFallbackModel = useCallback((modelId: string | null) => {
    const next = modelId && models.some((model) => model.id === modelId && isChatModel(model)) ? modelId : "";
    setStoredFallbackModelId(next);
    if (next) localStorage.setItem(FALLBACK_MODEL_KEY, next);
    else localStorage.removeItem(FALLBACK_MODEL_KEY);
  }, [models]);

  const resolve = useCallback(
    (content: string, hasImages = false) => routeModel(
      models,
      content,
      strategy,
      fallbackModelId || currentModelId,
      hasImages,
      assignments[strategy]
    ),
    [assignments, currentModelId, fallbackModelId, models, strategy]
  );

  return {
    enabled: effectiveEnabled,
    strategy,
    mode: effectiveEnabled ? strategy : "manual",
    assignments,
    fixedModelIds,
    fallbackModelId,
    isStrategyAvailable,
    setMode,
    setStrategyModel,
    setFixedModels,
    setFallbackModel,
    resolve
  };
}
