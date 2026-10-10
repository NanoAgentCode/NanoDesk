import { invoke } from "@tauri-apps/api/core";
export interface AbsoluteFileContent {
  name: string;
  size: number;
  content: string;
}
import type { UploadedFileExtractionRequest, RagChunkMatch, RagFile, RagFileDraft } from "../types";

export function listRagFiles(conversationId: string) {
  return invoke<RagFile[]>("list_rag_files", { conversationId });
}

export function indexRagFile(draft: RagFileDraft) {
  return invoke<RagFile>("index_rag_file", { draft });
}

export function deleteRagFile(id: string) {
  return invoke<void>("delete_rag_file", { id });
}

export function searchRagContext(conversationId: string, query: string, modelConfigId: string, limit = 6) {
  return invoke<RagChunkMatch[]>("search_rag_context", {
    conversationId,
    query,
    modelConfigId,
    limit
  });
}

export function readAbsoluteFile(path: string) {
  return invoke<AbsoluteFileContent>("read_absolute_file", { path });
}

export function extractUploadedFile(request: UploadedFileExtractionRequest) {
  return invoke<AbsoluteFileContent>("extract_uploaded_file", { request });
}
