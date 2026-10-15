import { useState } from "react";
import { listAgentRunTimelines } from "../api";
import type { AgentRunTimeline } from "../types";

export interface UseAgentRuntimeReturn {
  agentRunTimelines: AgentRunTimeline[];
  activeRunTimeline: AgentRunTimeline | null;
  expandedRuntimeRows: string[];
  showChatRuntime: boolean;
  setShowChatRuntime: React.Dispatch<React.SetStateAction<boolean>>;
  refreshRuntime: () => Promise<void>;
  toggleTimelineRow: (id: string) => void;
}

export function useAgentRuntime(
  setNotice: (message: string) => void,
  activeConversationId: string
): UseAgentRuntimeReturn {
  const [agentRunTimelines, setAgentRunTimelines] = useState<AgentRunTimeline[]>([]);
  const [expandedRuntimeRows, setExpandedRuntimeRows] = useState<string[]>([]);
  const [showChatRuntime, setShowChatRuntime] = useState(false);

  async function refreshRuntime() {
    try {
      setAgentRunTimelines(activeConversationId ? await listAgentRunTimelines(activeConversationId, 20) : []);
    } catch (error) {
      setNotice(String(error));
    }
  }

  function toggleTimelineRow(id: string) {
    setExpandedRuntimeRows((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id]
    );
  }

  return {
    agentRunTimelines,
    activeRunTimeline: agentRunTimelines[0] || null,
    expandedRuntimeRows,
    showChatRuntime,
    setShowChatRuntime,
    refreshRuntime,
    toggleTimelineRow
  };
}
