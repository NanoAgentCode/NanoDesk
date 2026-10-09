import { ActionIcon, Text, Tooltip } from "@mantine/core";
import { Loader2, Mic, Square, X } from "lucide-react";
import type { UseSpeechInputReturn } from "../hooks/useSpeechInput";

export default function SpeechInput({ disabled, speech }: { disabled: boolean; speech: UseSpeechInputReturn }) {
  const recording = speech.status === "recording";
  const working = speech.status === "starting" || speech.status === "transcribing";
  const label = recording ? "停止录音并转文字" : working ? "语音处理中" : "录音转文字";
  return (
    <>
      {recording && <Text size="xs" c="red" role="status">录音中 {speech.seconds}s</Text>}
      {working && <Text size="xs" c="dimmed" role="status">{speech.status === "starting" ? "启动麦克风…" : "识别中…"}</Text>}
      <Tooltip label={label} openDelay={450}>
        <ActionIcon className="chat-header-square ghost" aria-label={label} variant="subtle" color={recording ? "red" : undefined} disabled={disabled || working} onClick={() => void (recording ? speech.finishRecording() : speech.beginRecording())}>
          {working ? <Loader2 size={20} className="svg-spin" /> : recording ? <Square size={18} /> : <Mic size={20} />}
        </ActionIcon>
      </Tooltip>
      {speech.status !== "idle" && (
        <Tooltip label="取消语音输入" openDelay={450}>
          <ActionIcon className="chat-header-square ghost" aria-label="取消语音输入" variant="subtle" onClick={speech.cancel}><X size={18} /></ActionIcon>
        </Tooltip>
      )}
    </>
  );
}
