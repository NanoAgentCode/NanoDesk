import { useState } from "react";
import { saveChatImageAttachment } from "../api";
import {
  fileToBase64,
  isSupportedImageAttachment,
  isSupportedImageAttachmentFile
} from "../lib/imageAttachments";
import type { ChatImageAttachment } from "../types";

interface UseChatAttachmentsArgs {
  getProjectPath: () => string;
  onNotice: (message: string) => void;
  onDragEnd: () => void;
  getScopeKey: () => string;
}

export function buildImageAttachmentPrompt(attachments: ChatImageAttachment[]) {
  if (attachments.length === 0) return;
  const lines = [
    "图片附件：",
    ...attachments.map((attachment) => `- ${attachment.name}: ${attachment.relative_path}`),
    "需要识别图片文字时，请调用 ocr_image 工具。"
  ];
  return lines.join("\n");
}

export function buildMessageContentWithImageAttachments(
  textContent: string,
  attachments: ChatImageAttachment[]
) {
  const imagePrompt = buildImageAttachmentPrompt(attachments);
  if (!imagePrompt) return textContent;
  return textContent ? `${textContent}\n\n${imagePrompt}` : imagePrompt;
}

export function useChatAttachments({ getProjectPath, onNotice, onDragEnd, getScopeKey }: UseChatAttachmentsArgs) {
  const [uploadingImageAttachment, setUploadingImageAttachment] = useState(false);
  const [pendingImageAttachments, setPendingImageAttachments] = useState<ChatImageAttachment[]>([]);

  function addPendingImageAttachments(attachments: ChatImageAttachment[]) {
    if (attachments.length === 0) return;
    setPendingImageAttachments((current) => [...current, ...attachments]);
  }

  function removePendingImageAttachment(relativePath: string) {
    setPendingImageAttachments((current) =>
      current.filter((attachment) => attachment.relative_path !== relativePath)
    );
  }

  function clearPendingImageAttachments() {
    setPendingImageAttachments([]);
  }

  async function handleImageFiles(files: FileList | File[]) {
    const selectedFiles = Array.from(files).filter(isSupportedImageAttachmentFile);
    if (selectedFiles.length === 0) {
      onNotice("OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff。");
      return 0;
    }

    setUploadingImageAttachment(true);
    const scopeKey = getScopeKey();
    try {
      const attachments: ChatImageAttachment[] = [];
      const projectPath = getProjectPath();
      for (const file of selectedFiles) {
        const contentBase64 = await fileToBase64(file);
        const attachment = await saveChatImageAttachment({
          project_path: projectPath,
          file_name: file.name || "pasted-image.png",
          content_base64: contentBase64,
          source_path: null
        });
        attachments.push(attachment);
      }
      if (scopeKey !== getScopeKey()) return 0;
      addPendingImageAttachments(attachments);
      onNotice(`已添加 ${attachments.length} 张图片，可直接让助手识别文字。`);
      return attachments.length;
    } catch (error) {
      console.error("Failed to attach image:", error);
      if (scopeKey === getScopeKey()) onNotice(`图片添加失败：${String(error)}`);
      return 0;
    } finally {
      setUploadingImageAttachment(false);
      onDragEnd();
    }
  }

  async function attachDroppedImagePaths(paths: string[]) {
    const imagePaths = paths.filter((path) => isSupportedImageAttachment(path));
    if (imagePaths.length === 0) return 0;

    setUploadingImageAttachment(true);
    const scopeKey = getScopeKey();
    try {
      const attachments: ChatImageAttachment[] = [];
      const projectPath = getProjectPath();
      for (const filePath of imagePaths) {
        const fileName = filePath.split(/[/\\]/).pop() || "image.png";
        const attachment = await saveChatImageAttachment({
          project_path: projectPath,
          file_name: fileName,
          content_base64: null,
          source_path: filePath
        });
        attachments.push(attachment);
      }
      if (scopeKey !== getScopeKey()) return 0;
      addPendingImageAttachments(attachments);
      return attachments.length;
    } finally {
      setUploadingImageAttachment(false);
    }
  }

  return {
    uploadingImageAttachment,
    pendingImageAttachments,
    clearPendingImageAttachments,
    removePendingImageAttachment,
    handleImageFiles,
    attachDroppedImagePaths
  };
}
