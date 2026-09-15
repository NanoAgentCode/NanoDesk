import { useCallback, useState } from "react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import {
  isRoutingStrategy,
  routeModel,
  type ModelRoutingDecision,
  type RoutingStrategy
} from "../lib/modelRouting";
import type { ModelConfig } from "../types";

const ROUTING_ENABLED_KEY = `${APP_STORAGE_PREFIX}-smart-routing-enabled`;
const ROUTING_STRATEGY_KEY = `${APP_STORAGE_PREFIX}-smart-routing-strategy`;

export interface UseModelRoutingReturn {
  enabled: boolean;
  strategy: RoutingStrategy;
  mode: "manual" | RoutingStrategy;
  setMode: (mode: string | null) => void;
  resolve: (content: string, hasImages?: boolean) => ModelRoutingDecision | null;
}

export function useModelRouting(models: ModelConfig[], fallbackModelId: string): UseModelRoutingReturn {
  const [enabled, setEnabled] = useState(() => localStorage.getItem(ROUTING_ENABLED_KEY) === "true");
  const [strategy, setStrategy] = useState<RoutingStrategy>(() => {
    const stored = localStorage.getItem(ROUTING_STRATEGY_KEY);
    return isRoutingStrategy(stored) ? stored : "balanced";
  });

  const setMode = useCallback((mode: string | null) => {
    const nextEnabled = isRoutingStrategy(mode);
    setEnabled(nextEnabled);
    localStorage.setItem(ROUTING_ENABLED_KEY, String(nextEnabled));
    if (nextEnabled) {
      setStrategy(mode);
      localStorage.setItem(ROUTING_STRATEGY_KEY, mode);
    }
  }, []);

  const resolve = useCallback(
    (content: string, hasImages = false) => routeModel(models, content, strategy, fallbackModelId, hasImages),
    [fallbackModelId, models, strategy]
  );

  return { enabled, strategy, mode: enabled ? strategy : "manual", setMode, resolve };
}
