import { useRef } from "react";
import { ActionIcon, Tooltip } from "@mantine/core";
import { Loader2, Paperclip } from "lucide-react";
import { ATTACHMENT_ACCEPT } from "../lib/attachmentUploads";
import type { UseSpeechInputReturn } from "../hooks/useSpeechInput";
import SpeechInput from "./SpeechInput";

export default function ChatAttachmentActions({ disabled, uploading, speech, onFiles }: {
  disabled: boolean;
  uploading: boolean;
  speech: UseSpeechInputReturn;
  onFiles: (files: File[]) => Promise<void>;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const uploadDisabled = disabled || uploading || speech.status !== "idle";
  return (
    <>
      <SpeechInput disabled={disabled || (uploading && speech.status === "idle")} speech={speech} />
      <input ref={inputRef} className="chat-image-input" type="file" accept={ATTACHMENT_ACCEPT} multiple disabled={uploadDisabled} onChange={(event) => {
        const files = Array.from(event.currentTarget.files || []);
        event.currentTarget.value = "";
        if (files.length) void onFiles(files);
      }} />
      <Tooltip label={uploading ? "附件处理中" : "添加附件（图片、音频、文档）"} openDelay={450}>
        <ActionIcon className="chat-header-square ghost" aria-label="添加附件" disabled={uploadDisabled} variant="subtle" onClick={() => inputRef.current?.click()}>
          {uploading ? <Loader2 size={20} className="svg-spin" /> : <Paperclip size={22} />}
        </ActionIcon>
      </Tooltip>
    </>
  );
}
