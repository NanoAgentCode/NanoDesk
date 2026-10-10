import { invoke } from "@tauri-apps/api/core";
import type { Memory, MemoryDraft, MemoryPatch } from "../types";

export function listMemories() {
  return invoke<Memory[]>("list_memories");
}

export function listEnabledMemories() {
  return invoke<Memory[]>("list_enabled_memories");
}

export function listRelevantMemories(query: string, limit = 8) {
  return invoke<Memory[]>("list_relevant_memories", { query, limit });
}

export function searchMemories(query: string) {
  return invoke<Memory[]>("search_memories", { query });
}

export function createMemory(draft: MemoryDraft) {
  return invoke<Memory>("create_memory", { draft });
}

export function updateMemory(patch: MemoryPatch) {
  return invoke<Memory>("update_memory", { patch });
}

export function deleteMemory(id: string) {
  return invoke<void>("delete_memory", { id });
}
