import { useState } from "react";
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

export function useRagFiles(setNotice: (message: string) => void): UseRagFilesReturn {
  const [ragFiles, setRagFiles] = useState<RagFile[]>([]);
  const [isRagDragging, setIsRagDragging] = useState(false);
  const [indexingRagFileName, setIndexingRagFileName] = useState("");

  async function refreshRagFiles(conversationId: string) {
    try {
      setRagFiles(await listRagFiles(conversationId));
    } catch (error) {
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
    ragFiles,
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
