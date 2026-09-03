import { useEffect, useMemo, useState } from "react";
import { FolderOpen, X } from "lucide-react";
import MarkdownMessage from "./MarkdownMessage";
import { openProjectFileLocation, readChatImageAttachment } from "../api";
import type { ProjectFileEntry } from "../types";
import { PROJECT_DATA_DIRECTORY } from "../config/brand";

interface ImageAttachmentMessageProps {
  content: string;
  projectPath?: string | null;
  projectFiles?: ProjectFileEntry[];
}

interface ParsedImageAttachment {
  name: string;
  relativePath: string;
}

interface LoadedImageAttachment extends ParsedImageAttachment {
  url: string | null;
  absolutePath?: string;
  error?: string;
}

const escapedProjectDataDirectory = PROJECT_DATA_DIRECTORY.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const IMAGE_ATTACHMENT_LINE = new RegExp(`^-\\s+(.+?):\\s+(${escapedProjectDataDirectory}/uploads/images/.+)$`);
const IMAGE_ATTACHMENT_HINT = "需要识别图片文字时，请调用 ocr_image 工具。";

function parseImageAttachmentContent(content: string) {
  const displayLines: string[] = [];
  const attachments: ParsedImageAttachment[] = [];
  const lines = content.split(/\r?\n/);
  let inAttachmentBlock = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "图片附件：") {
      inAttachmentBlock = true;
      continue;
    }
    if (inAttachmentBlock && trimmed === IMAGE_ATTACHMENT_HINT) {
      inAttachmentBlock = false;
      continue;
    }
    if (inAttachmentBlock) {
      const match = IMAGE_ATTACHMENT_LINE.exec(trimmed);
      if (match) {
        attachments.push({
          name: match[1],
          relativePath: match[2]
        });
        continue;
      }
      inAttachmentBlock = false;
    }
    displayLines.push(line);
  }

  return {
    displayContent: displayLines.join("\n").trim(),
    attachments
  };
}

export default function ImageAttachmentMessage({ content, projectPath, projectFiles = [] }: ImageAttachmentMessageProps) {
  const [preview, setPreview] = useState<LoadedImageAttachment | null>(null);
  const [loadedAttachments, setLoadedAttachments] = useState<LoadedImageAttachment[]>([]);
  const parsed = useMemo(() => parseImageAttachmentContent(content), [content]);

  useEffect(() => {
    let cancelled = false;
    setLoadedAttachments(parsed.attachments.map((attachment) => ({ ...attachment, url: null })));

    async function loadAttachments() {
      if (!projectPath || parsed.attachments.length === 0) {
        return;
      }

      const nextAttachments = await Promise.all(parsed.attachments.map(async (attachment) => {
        try {
          const previewImage = await readChatImageAttachment(projectPath, attachment.relativePath);
          return { ...attachment, url: previewImage.data_url, absolutePath: previewImage.absolute_path };
        } catch (error) {
          console.error("Failed to preview image attachment:", error);
          return { ...attachment, url: null, error: String(error) };
        }
      }));
      if (!cancelled) {
        setLoadedAttachments(nextAttachments);
      }
    }

    void loadAttachments();
    return () => {
      cancelled = true;
    };
  }, [parsed.attachments, projectPath]);

  if (parsed.attachments.length === 0) {
    return <MarkdownMessage content={content} projectPath={projectPath} projectFiles={projectFiles} />;
  }

  return (
    <>
      {parsed.displayContent && (
        <MarkdownMessage content={parsed.displayContent} projectPath={projectPath} projectFiles={projectFiles} />
      )}
      <div className="image-attachment-grid">
        {loadedAttachments.map((attachment) => (
          <div
            key={`${attachment.relativePath}-${attachment.name}`}
            className="image-attachment-card"
            title={attachment.absolutePath || attachment.relativePath}
          >
            <button
              className="image-attachment-thumb"
              type="button"
              onClick={() => attachment.url && setPreview(attachment)}
              disabled={!attachment.url}
              aria-label={`预览图片 ${attachment.name}`}
            >
              {attachment.url ? (
                <img src={attachment.url} alt={attachment.name} loading="lazy" />
              ) : (
                <span>{attachment.error ? "预览失败" : attachment.name}</span>
              )}
            </button>
            <div className="image-attachment-meta">
              <span>{attachment.name}</span>
              <code>{attachment.absolutePath || attachment.relativePath}</code>
            </div>
            <button
              className="image-attachment-open"
              type="button"
              disabled={!projectPath || Boolean(attachment.error)}
              onClick={() => {
                if (!projectPath) return;
                void openProjectFileLocation(projectPath, attachment.relativePath).catch((error) => {
                  console.error("Failed to open image attachment location:", error);
                });
              }}
              aria-label={`在资源管理器中打开 ${attachment.name} 所在文件夹`}
              title="打开所在文件夹"
            >
              <FolderOpen size={15} />
            </button>
          </div>
        ))}
      </div>
      {preview?.url && (
        <div className="image-preview-backdrop" onClick={() => setPreview(null)} role="presentation">
          <div className="image-preview-dialog" onClick={(event) => event.stopPropagation()}>
            <button
              className="image-preview-close"
              type="button"
              onClick={() => setPreview(null)}
              aria-label="关闭预览"
              title="关闭"
            >
              <X size={18} />
            </button>
            <img src={preview.url} alt={preview.name} />
            <div className="image-preview-footer">
              <code>{preview.absolutePath || preview.relativePath}</code>
              <button
                type="button"
                onClick={() => {
                  if (!projectPath) return;
                  void openProjectFileLocation(projectPath, preview.relativePath).catch((error) => {
                    console.error("Failed to open image attachment location:", error);
                  });
                }}
                disabled={!projectPath}
              >
                <FolderOpen size={15} />
                打开所在文件夹
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
