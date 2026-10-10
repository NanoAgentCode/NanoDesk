import { invoke } from "@tauri-apps/api/core";
import type { Item, ItemDraft, ItemPatch } from "../types";

export function listItems(kind?: string) {
  return invoke<Item[]>("list_items", { kind: kind || null });
}

export function searchItems(query: string) {
  return invoke<Item[]>("search_items", { query });
}

export function createItem(draft: ItemDraft) {
  return invoke<Item>("create_item", { draft });
}

export function updateItem(patch: ItemPatch) {
  return invoke<Item>("update_item", { patch });
}

export function deleteItem(id: string) {
  return invoke<void>("delete_item", { id });
}
