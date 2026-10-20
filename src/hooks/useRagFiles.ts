import { useEffect, useRef, useState } from "react";
import {
  deleteRagFile,
  indexRagFile,
  listRagFiles,
  readAbsoluteFile,
  searchRagContext
} from "../api";
import { isSupportedRagFile } from "../lib/formatters";
import { confirmAction } from "../lib/dialogs";
import type { RagChunkMatch, RagFile } from "../types";

export interface UseRagFilesReturn {
  ragFiles: RagFile[];
  setRagFiles: React.Dispatch<React.SetStateAction<RagFile[]>>;
  isRagDragging: boolean;
  setIsRagDragging: React.Dispatch<React.SetStateAction<boolean>>;
  indexingRagFileName: string;
  setIndexingRagFileName: React.Dispatch<React.SetStateAction<string>>;
  refreshRagFiles: (conversationId: string) => Promise<void>;
  loadRagMatches: (conversationId: string, queryText: string, modelConfigId: string) => Promise<RagChunkMatch[]>;
  handleDeleteRagFile: (id: string, conversationId: string) => Promise<void>;
}

export function useRagFiles(setNotice: (message: string) => void, conversationId: string): UseRagFilesReturn {
  const scope = useRef({ id: conversationId, revision: 0 });
  if (scope.current.id !== conversationId) scope.current = { id: conversationId, revision: scope.current.revision + 1 };
  const [ragFiles, setRagFiles] = useState<RagFile[]>([]);
  const [isRagDragging, setIsRagDragging] = useState(false);
  const [indexingRagFileName, setIndexingRagFileName] = useState("");
  useEffect(() => () => { scope.current.revision++; }, []);
  const pendingConversation = ragFiles.find(file => ["queued", "preparing", "reducing"].includes(file.preparation?.phase || ""))?.conversation_id;
  useEffect(() => {
    if (!pendingConversation) return;
    let disposed = false;
    const timer = setInterval(() => {
      const revision = scope.current.revision;
      void listRagFiles(pendingConversation).then(files => {
        if (!disposed && scope.current.id === pendingConversation && scope.current.revision === revision) setRagFiles(current => current.length > 0 && current.every(file => file.conversation_id === pendingConversation) ? files : current);
      }).catch(console.error);
    }, 2000);
    return () => { disposed = true; clearInterval(timer); };
  }, [pendingConversation]);

  async function refreshRagFiles(conversationId: string) {
    if (scope.current.id !== conversationId) return;
    const revision = ++scope.current.revision;
    try {
      const files = await listRagFiles(conversationId);
      if (scope.current.revision === revision) setRagFiles(files);
    } catch (error) {
      if (scope.current.revision !== revision) return;
      console.error("Failed to list RAG files:", error);
      setRagFiles([]);
    }
  }

  async function loadRagMatches(
    conversationId: string,
    queryText: string,
    modelConfigId: string
  ): Promise<RagChunkMatch[]> {
    if (!conversationId || !queryText.trim() || !modelConfigId || ragFiles.length === 0) {
      return [];
    }
    try {
      return await searchRagContext(conversationId, queryText, modelConfigId, 6);
    } catch (error) {
      console.error("Failed to search RAG context:", error);
      setNotice(`文件检索失败，将跳过 RAG 上下文：${String(error)}`);
      return [];
    }
  }

  async function handleDeleteRagFile(id: string, conversationId: string) {
    const target = ragFiles.find((file) => file.id === id);
    if (!(await confirmAction(`确定要移除「${target?.name || "该文件"}」的索引吗？原文件不会被删除。`))) {
      return;
    }
    try {
      await deleteRagFile(id);
      if (conversationId) {
        await refreshRagFiles(conversationId);
      }
    } catch (error) {
      console.error("Failed to delete RAG file:", error);
      setNotice(`删除文件索引失败：${String(error)}`);
    }
  }

  return {
    ragFiles: ragFiles.filter(file => file.conversation_id === conversationId),
    setRagFiles,
    isRagDragging,
    setIsRagDragging,
    indexingRagFileName,
    setIndexingRagFileName,
    refreshRagFiles,
    loadRagMatches,
    handleDeleteRagFile
  };
}
