import { isSupportedRagFile } from "./formatters";
import { isSupportedImageAttachment, isSupportedImageAttachmentFile } from "./imageAttachments";
import { AUDIO_ACCEPT, isSupportedAudioAttachment } from "./speech";

export type AttachmentSource = { kind: "file"; file: File } | { kind: "path"; path: string };
export type AttachmentKind = "image" | "audio" | "document" | "unsupported";
export const ATTACHMENT_ACCEPT = [
  "image/png,image/jpeg,image/bmp,image/webp,image/tiff", AUDIO_ACCEPT,
  ".txt,.md,.markdown,.json,.csv,.tsv,.log,.js,.jsx,.ts,.tsx,.rs,.py,.java,.go,.yaml,.yml,.toml,.html,.css,.xml,.pdf,.doc,.docx,.xlsx,.pptx"
].join(",");

export function attachmentName(source: AttachmentSource): string {
  return source.kind === "file" ? source.file.name : source.path.split(/[/\\]/).pop() || "unknown";
}

export function classifyAttachment(source: AttachmentSource): AttachmentKind {
  const name = attachmentName(source);
  if (source.kind === "file" ? isSupportedImageAttachmentFile(source.file) : isSupportedImageAttachment(name)) return "image";
  if (isSupportedAudioAttachment(name)) return "audio";
  if (isSupportedRagFile(name)) return "document";
  return "unsupported";
}

export function partitionAttachments(sources: AttachmentSource[]) {
  const groups: Record<AttachmentKind, AttachmentSource[]> = { image: [], audio: [], document: [], unsupported: [] };
  for (const source of sources) groups[classifyAttachment(source)].push(source);
  return groups;
}

export function formatAttachmentUploadResult(result: { image: number; audio: number; document: number; errors: string[]; unsupported: string[] }) {
  const successes = [];
  if (result.image) successes.push(`已添加 ${result.image} 张图片`);
  if (result.audio) successes.push(`已识别 ${result.audio} 个音频，文字已追加到输入框`);
  if (result.document) successes.push(`已索引 ${result.document} 个文档到当前对话`);
  const messages = successes.length ? [successes.join("，") + "。"] : [];
  if (result.errors.length) messages.push(`处理失败：${result.errors.join("；")}`);
  if (result.unsupported.length) messages.push(`不支持的文件：${result.unsupported.join("、")}`);
  return messages.join("\n") || "请选择图片、音频或文档附件。";
}
