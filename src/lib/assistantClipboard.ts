import { classifyAttachment } from "./attachmentUploads";
import { fileToDataUrl } from "./imageAttachments";

export interface AssistantClipboardFile { name: string; type: string; dataUrl: string }

export async function prepareAssistantClipboard(files: File[], readPaths: () => Promise<string[]>) {
  const paths = await readPaths();
  const supportedPaths = paths.filter(path => classifyAttachment({ kind: "path", path }) !== "unsupported");
  if (paths.length) return { paths: supportedPaths, files: [] as AssistantClipboardFile[], unsupported: paths.length - supportedPaths.length };
  const supported = files.filter(file => classifyAttachment({ kind: "file", file }) !== "unsupported");
  const uploads = await Promise.all(supported.map(async file => ({ name: file.name, type: file.type, dataUrl: await fileToDataUrl(file) })));
  return { paths: [], files: uploads, unsupported: files.length - supported.length };
}

export function restoreAssistantClipboardFile(file: AssistantClipboardFile) {
  const encoded = file.dataUrl.slice(file.dataUrl.indexOf(",") + 1);
  const bytes = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
  return new File([bytes], file.name, { type: file.type });
}
