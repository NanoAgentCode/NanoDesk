import { useEffect, useState, useMemo, useRef } from "react";
import {
  searchItems,
  listItems,
  createItem,
  updateItem,
  deleteItem
} from "../api";
import type { Item, ItemKind, WorkspaceView } from "../types";
import { parseTags } from "../lib/messageHelpers";
import { confirmAction } from "../lib/dialogs";
import type { UseMemoryReturn } from "./useMemory";

const kindLabels: Record<ItemKind, string> = {
  note: "笔记",
  prompt: "提示词"
};

export interface UseWorkspaceReturn {
  items: Item[];
  setItems: React.Dispatch<React.SetStateAction<Item[]>>;
  selectedId: string;
  setSelectedId: React.Dispatch<React.SetStateAction<string>>;
  activeKind: WorkspaceView;
  setActiveKind: React.Dispatch<React.SetStateAction<WorkspaceView>>;
  query: string;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  title: string;
  setTitle: React.Dispatch<React.SetStateAction<string>>;
  body: string;
  setBody: React.Dispatch<React.SetStateAction<string>>;
  tagsText: string;
  setTagsText: React.Dispatch<React.SetStateAction<string>>;
  status: string;
  setStatus: React.Dispatch<React.SetStateAction<string>>;
  selectedItem: Item | undefined;
  refreshItems: (nextQuery?: string, kind?: WorkspaceView) => Promise<void>;
  handleKindChange: (kind: WorkspaceView) => void;
  handleSearch: (value: string) => void;
  handleNewItem: (kind: ItemKind) => Promise<void>;
  handleSaveItem: () => Promise<void>;
  handleDeleteItem: () => Promise<void>;
}

export function useWorkspace(
  setNotice: (message: string) => void,
  memory: UseMemoryReturn
): UseWorkspaceReturn {
  const listRequestRef = useRef(0);
  const [items, setItems] = useState<Item[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [activeKind, setActiveKind] = useState<WorkspaceView>("note");
  const [query, setQuery] = useState("");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [tagsText, setTagsText] = useState("");
  const [status, setStatus] = useState("active");

  const selectedItem = useMemo(
    () => items.find((item) => item.id === selectedId),
    [items, selectedId]
  );

  useEffect(() => {
    if (!selectedItem) {
      setTitle("");
      setBody("");
      setTagsText("");
      setStatus("active");
      return;
    }

    setTitle(selectedItem.title);
    setBody(selectedItem.body);
    setTagsText(selectedItem.tags.join(", "));
    setStatus(selectedItem.status);
  }, [selectedItem]);

  useEffect(() => {
    void refreshItems(query, activeKind);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeKind, query]);

  async function refreshItems(nextQuery = query, kind = activeKind) {
    const requestId = ++listRequestRef.current;

    try {
      if (kind === "memory") {
        await memory.refreshMemories(nextQuery);
        return;
      }

      const nextItems = nextQuery.trim()
        ? await searchItems(nextQuery)
        : await listItems(kind === "all" ? undefined : kind);
      if (requestId !== listRequestRef.current) {
        return;
      }
      setItems(nextItems);
      setSelectedId((current) =>
        nextItems.some((item) => item.id === current)
          ? current
          : nextItems[0]?.id || ""
      );
    } catch (error) {
      setNotice(String(error));
    }
  }

  function handleKindChange(kind: WorkspaceView) {
    setActiveKind(kind);
    setQuery("");
    if (kind === "memory") {
      memory.setSelectedMemoryId("");
    } else {
      setSelectedId("");
    }
  }

  function handleSearch(value: string) {
    setQuery(value);
  }

  async function handleNewItem(kind: ItemKind) {
    const item = await createItem({
      kind,
      title: `新建${kindLabels[kind]}`,
      body: "",
      status: "active",
      tags: []
    });
    setActiveKind(kind);
    setQuery("");
    await refreshItems("", kind);
    setSelectedId(item.id);
  }

  async function handleSaveItem() {
    if (!selectedItem) {
      return;
    }

    await updateItem({
      id: selectedItem.id,
      title,
      body,
      status,
      tags: parseTags(tagsText)
    });

    await refreshItems(query, activeKind);
    setNotice("已保存");
  }

  async function handleDeleteItem() {
    if (!selectedItem) {
      return;
    }

    const kindLabel = kindLabels[selectedItem.kind as ItemKind] ?? selectedItem.kind;
    if (!(await confirmAction(`确定要删除${kindLabel}「${selectedItem.title}」吗？`))) {
      return;
    }
    await deleteItem(selectedItem.id);
    setSelectedId("");
    await refreshItems(query, activeKind);
  }

  return {
    items,
    setItems,
    selectedId,
    setSelectedId,
    activeKind,
    setActiveKind,
    query,
    setQuery,
    title,
    setTitle,
    body,
    setBody,
    tagsText,
    setTagsText,
    status,
    setStatus,
    selectedItem,
    refreshItems,
    handleKindChange,
    handleSearch,
    handleNewItem,
    handleSaveItem,
    handleDeleteItem
  };
}
