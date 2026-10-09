import { describe, expect, it } from "vitest";
import { classifyAttachment, partitionAttachments, formatAttachmentUploadResult } from "./attachmentUploads";

describe("unified attachment uploads", () => {
  it("routes browser files and native paths to the same image, audio and document purposes", () => {
    expect(classifyAttachment({ kind: "path", path: "C:\\recordings\\VOICE.WAV" })).toBe("audio");
    expect(classifyAttachment({ kind: "path", path: "C:\\docs\\report.pdf" })).toBe("document");
    expect(classifyAttachment({ kind: "path", path: "C:\\images\\photo.PNG" })).toBe("image");
    expect(classifyAttachment({ kind: "file", file: new File(["sound"], "voice.mp3", { type: "audio/mpeg" }) })).toBe("audio");
    expect(classifyAttachment({ kind: "file", file: new File(["img"], "clipboard", { type: "image/png" }) })).toBe("image");
    expect(classifyAttachment({ kind: "path", path: "C:\\program.exe" })).toBe("unsupported");
  });

  it("partitions mixed drops without sending audio to document extraction and retains audio order", () => {
    const sources = ["a.wav", "photo.jpg", "report.docx", "b.MP3", "unknown.bin"].map((path) => ({ kind: "path" as const, path }));
    const grouped = partitionAttachments(sources);
    expect(grouped.audio).toEqual([sources[0], sources[3]]);
    expect(grouped.document).toEqual([sources[2]]);
    expect(grouped.image).toEqual([sources[1]]);
    expect(grouped.unsupported).toEqual([sources[4]]);
  });

  it("reports successful uploads together with failures without hiding ASR errors", () => {
    const result = formatAttachmentUploadResult({ image: 1, audio: 0, document: 1, errors: ["voice.wav：请先选择语音识别模型"], unsupported: ["program.exe"] });
    expect(result).toContain("1 张图片");
    expect(result).toContain("1 个文档");
    expect(result).toContain("voice.wav");
    expect(result).toContain("语音识别模型");
    expect(result).toContain("program.exe");
  });
});
