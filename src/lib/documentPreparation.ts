import type { RagFile, RagFileDraft } from "../types";
import type { DocumentSource } from "./documentChat";

interface Preparation {
  conversationId: string;
  modelConfigId: string;
  modelRevision?: string;
  source: DocumentSource;
  mime?: string;
  size?: number;
  existingFiles: RagFile[];
}

/** Entry points share preparation; only successful, still-attached results are reused. */
export function createDocumentPreparer(index: (draft: RagFileDraft) => Promise<RagFile>) {
  const cache = new WeakMap<DocumentSource, {
    conversationId: string; modelConfigId: string; modelRevision: string; content: string; name: string;
    pending: Promise<RagFile>; file?: RagFile;
  }>();
  return async ({ conversationId, modelConfigId, modelRevision = "", source, mime = "text/plain", size, existingFiles }: Preparation) => {
    const previous = cache.get(source);
    if (previous?.conversationId === conversationId && previous.modelConfigId === modelConfigId
      && previous.modelRevision === modelRevision && previous.content === source.content && previous.name === source.name
      && (!previous.file || existingFiles.some(file => file.id === previous.file!.id && file.preparation?.phase !== "failed"))) return previous.pending;
    const pending = index({ conversation_id: conversationId, model_config_id: modelConfigId,
      name: source.name, content: source.content, mime,
      size: size ?? new TextEncoder().encode(source.content).length });
    const entry = { conversationId, modelConfigId, modelRevision, content: source.content, name: source.name, pending, file: undefined as RagFile | undefined };
    cache.set(source, entry);
    try { entry.file = await pending; return entry.file; }
    catch (error) { if (cache.get(source) === entry) cache.delete(source); throw error; }
  };
}
