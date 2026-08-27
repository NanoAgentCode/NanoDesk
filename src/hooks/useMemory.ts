import { useEffect, useState, useMemo } from "react";
import {
  listMemories,
  listEnabledMemories,
  searchMemories,
  updateMemory,
  deleteMemory
} from "../api";
import type { Memory } from "../types";
import { parseTags } from "../lib/messageHelpers";
import { confirmAction } from "../lib/dialogs";

export interface UseMemoryReturn {
  memoryItems: Memory[];
  setMemoryItems: React.Dispatch<React.SetStateAction<Memory[]>>;
  selectedMemoryId: string;
  setSelectedMemoryId: React.Dispatch<React.SetStateAction<string>>;
  memoryTitle: string;
  setMemoryTitle: React.Dispatch<React.SetStateAction<string>>;
  memoryContent: string;
  setMemoryContent: React.Dispatch<React.SetStateAction<string>>;
  memoryTagsText: string;
  setMemoryTagsText: React.Dispatch<React.SetStateAction<string>>;
  memoryEnabled: boolean;
  setMemoryEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  selectedMemory: Memory | null;
  loadVisibleMemories: (nextQuery?: string) => Promise<Memory[]>;
  refreshMemories: (query?: string) => Promise<Memory[]>;
  handleMemoryCreated: (createdMemory: Memory) => void;
  handleSaveMemory: (query: string) => Promise<void>;
  handleDeleteMemory: (query: string) => Promise<void>;
}

export function mergeCreatedMemory(memoryItems: Memory[], createdMemory: Memory) {
  return [createdMemory, ...memoryItems.filter((memory) => memory.id !== createdMemory.id)];
}

export function useMemory(setNotice: (message: string) => void): UseMemoryReturn {
  const [memoryItems, setMemoryItems] = useState<Memory[]>([]);
  const [selectedMemoryId, setSelectedMemoryId] = useState("");
  const [memoryTitle, setMemoryTitle] = useState("");
  const [memoryContent, setMemoryContent] = useState("");
  const [memoryTagsText, setMemoryTagsText] = useState("");
  const [memoryEnabled, setMemoryEnabled] = useState(true);

  const selectedMemory = useMemo(
    () => memoryItems.find((memory) => memory.id === selectedMemoryId) || null,
    [memoryItems, selectedMemoryId]
  );

  useEffect(() => {
    if (!selectedMemory) {
      setMemoryTitle("");
      setMemoryContent("");
      setMemoryTagsText("");
      setMemoryEnabled(true);
      return;
    }

    setMemoryTitle(selectedMemory.title);
    setMemoryContent(selectedMemory.content);
    setMemoryTagsText(selectedMemory.tags.join(", "));
    setMemoryEnabled(selectedMemory.enabled);
  }, [selectedMemory]);

  async function loadVisibleMemories(nextQuery = "") {
    if (nextQuery.trim()) {
      return searchMemories(nextQuery);
    }

    const allMemories = await listMemories();
    if (allMemories.length > 0) {
      return allMemories;
    }

    return listEnabledMemories();
  }

  async function refreshMemories(query = "") {
    try {
      const nextMemories = await loadVisibleMemories(query);
      setMemoryItems(nextMemories);
      setSelectedMemoryId((current) =>
        nextMemories.some((memory) => memory.id === current)
          ? current
          : nextMemories[0]?.id || ""
      );
      return nextMemories;
    } catch (e) {
      setNotice(String(e));
      return [];
    }
  }

  function handleMemoryCreated(createdMemory: Memory) {
    setMemoryItems((current) => mergeCreatedMemory(current, createdMemory));
    setSelectedMemoryId(createdMemory.id);
  }

  async function handleSaveMemory(query: string) {
    if (!selectedMemory) {
      return;
    }
    try {
      await updateMemory({
        id: selectedMemory.id,
        title: memoryTitle,
        content: memoryContent,
        tags: parseTags(memoryTagsText),
        enabled: memoryEnabled
      });
      setNotice("记忆已更新。");
      await refreshMemories(query);
    } catch (e) {
      setNotice(`保存记忆失败：${String(e)}`);
    }
  }

  async function handleDeleteMemory(query: string) {
    if (!selectedMemory) {
      return;
    }
    if (!(await confirmAction("确定要删除该记忆吗？"))) {
      return;
    }
    try {
      await deleteMemory(selectedMemory.id);
      setSelectedMemoryId("");
      setNotice("记忆已删除。");
      await refreshMemories(query);
    } catch (e) {
      setNotice(`删除记忆失败：${String(e)}`);
    }
  }

  return {
    memoryItems,
    setMemoryItems,
    selectedMemoryId,
    setSelectedMemoryId,
    memoryTitle,
    setMemoryTitle,
    memoryContent,
    setMemoryContent,
    memoryTagsText,
    setMemoryTagsText,
    memoryEnabled,
    setMemoryEnabled,
    selectedMemory,
    loadVisibleMemories,
    refreshMemories,
    handleMemoryCreated,
    handleSaveMemory,
    handleDeleteMemory
  };
}
