const IMAGE_ATTACHMENT_EXTENSIONS = new Set(["png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"]);

export function isSupportedImageAttachment(path: string) {
  const ext = path.split(".").pop()?.toLowerCase();
  return ext ? IMAGE_ATTACHMENT_EXTENSIONS.has(ext) : false;
}

export function isSupportedImageAttachmentFile(file: File) {
  if (isSupportedImageAttachment(file.name)) return true;
  if (!file.type.startsWith("image/")) return false;
  const subtype = file.type.slice("image/".length).toLowerCase();
  return IMAGE_ATTACHMENT_EXTENSIONS.has(subtype);
}

export function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result === "string") {
        resolve(result);
      } else {
        reject(new Error("文件读取失败"));
      }
    };
    reader.onerror = () => reject(reader.error || new Error("文件读取失败"));
    reader.readAsDataURL(file);
  });
}

export const fileToBase64 = fileToDataUrl;
