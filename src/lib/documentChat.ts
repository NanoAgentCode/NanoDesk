export interface DocumentSource { name: string; content: string }
export function buildDocumentReference(name: string, question: string) {
  return `[会话文档：${name}]\n${question}`;
}
const prefix = "[NanoDesk 文档 ";
const separator = "\n[/NanoDesk 文档]\n用户问题：";

export function buildDocumentMessage(source: DocumentSource, question: string) {
  return `${prefix}${JSON.stringify({ name: source.name, length: source.content.length })}]\n${source.content}${separator}${question}`;
}

export function splitDocumentMessage(content: string) {
  if (!content.startsWith(prefix)) return null;
  const start = content.indexOf("]\n");
  if (start < 0) return null;
  try {
    const header = JSON.parse(content.slice(prefix.length, start));
    if (typeof header.name !== "string" || !Number.isSafeInteger(header.length) || header.length < 0) return null;
    const end = start + 2 + header.length;
    if (!content.startsWith(separator, end)) return null;
    return { name: header.name as string, document: content.slice(start + 2, end), question: content.slice(end + separator.length) };
  } catch { return null; }
}
