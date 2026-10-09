import { useEffect, useRef, useState } from "react";
import { ActionIcon, Text, Tooltip } from "@mantine/core";
import { FileAudio, Loader2, Mic, Square, X } from "lucide-react";
import { getAsrConfig, transcribeAudio } from "../api";
import { fileToDataUrl } from "../lib/imageAttachments";
import { AUDIO_ACCEPT, recordingToWav, validateAudioFile } from "../lib/speech";
import { SpeechRecorder, startSpeechRecording } from "../lib/speechRecorder";

interface SpeechInputProps {
  disabled: boolean;
  onTranscript: (text: string) => void;
  setNotice: (message: string) => void;
}

export default function SpeechInput({ disabled, onTranscript, setNotice }: SpeechInputProps) {
  const [status, setStatus] = useState<"idle" | "starting" | "recording" | "transcribing">("idle");
  const [seconds, setSeconds] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const recorderRef = useRef<SpeechRecorder | null>(null);
  const operation = useRef(0);
  const locked = useRef(false);
  const latest = useRef({ disabled, onTranscript, setNotice });
  latest.current = { disabled, onTranscript, setNotice };

  function cancel() {
    operation.current++;
    recorderRef.current?.cancel();
    recorderRef.current = null;
    locked.current = false;
    setStatus("idle");
  }

  useEffect(() => {
    if (disabled) cancel();
    return () => {
      operation.current++;
      recorderRef.current?.cancel();
      recorderRef.current = null;
      locked.current = false;
    };
  }, [disabled]);

  useEffect(() => {
    if (status !== "recording") return;
    const timer = window.setInterval(() => setSeconds((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [status]);

  useEffect(() => {
    if (status === "recording" && seconds >= 300) void finishRecording();
  }, [seconds, status]);

  const valid = (id: number) => id === operation.current && !latest.current.disabled;
  function fail(error: unknown, id: number) {
    if (!valid(id)) return;
    const denied = error instanceof DOMException && error.name === "NotAllowedError";
    latest.current.setNotice(denied ? "麦克风权限被拒绝，请在系统隐私设置中允许访问麦克风" : `语音识别失败：${String(error)}`);
    cancel();
  }

  async function transcribe(file: File, id: number) {
    const error = validateAudioFile(file);
    if (error) throw new Error(error);
    if (!valid(id)) return;
    setStatus("transcribing");
    const data = await fileToDataUrl(file);
    if (!valid(id)) return;
    const text = await transcribeAudio(file.name, data.slice(data.indexOf(",") + 1));
    if (!valid(id)) return;
    latest.current.onTranscript(text);
    latest.current.setNotice("语音已转为文字，请检查后发送");
    locked.current = false;
    setStatus("idle");
  }

  async function beginRecording() {
    if (locked.current || disabled) return;
    locked.current = true;
    const id = ++operation.current;
    setStatus("starting");
    try {
      if (!(await getAsrConfig())) throw new Error("请先在系统设置 → 模型路由中选择语音识别模型");
      if (!valid(id)) return;
      const recorder = await startSpeechRecording((error) => fail(error, id));
      if (!valid(id)) { recorder.cancel(); return; }
      recorderRef.current = recorder;
      setSeconds(0);
      setStatus("recording");
    } catch (error) { fail(error, id); }
  }

  async function finishRecording() {
    const recorder = recorderRef.current;
    if (!recorder || status !== "recording") return;
    recorderRef.current = null;
    const id = operation.current;
    setStatus("transcribing");
    try {
      const blob = await recorder.stop();
      if (!valid(id)) return;
      await transcribe(await recordingToWav(blob), id);
    } catch (error) { fail(error, id); }
  }

  async function chooseAudio(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file || locked.current || disabled) return;
    locked.current = true;
    const id = ++operation.current;
    setStatus("transcribing");
    try { await transcribe(file, id); }
    catch (error) { fail(error, id); }
  }

  const recording = status === "recording";
  const working = status === "starting" || status === "transcribing";
  const label = recording ? "停止录音并转文字" : working ? "语音处理中" : "录音转文字";
  return (
    <>
      <input ref={inputRef} type="file" accept={AUDIO_ACCEPT} hidden disabled={disabled || status !== "idle"} onChange={(event) => void chooseAudio(event)} />
      {recording && <Text size="xs" c="red" role="status">录音中 {seconds}s</Text>}
      {working && <Text size="xs" c="dimmed" role="status">{status === "starting" ? "启动麦克风…" : "识别中…"}</Text>}
      <Tooltip label={label} openDelay={450}>
        <ActionIcon className="chat-header-square ghost" aria-label={label} variant="subtle" color={recording ? "red" : undefined} disabled={disabled || working} onClick={() => void (recording ? finishRecording() : beginRecording())}>
          {working ? <Loader2 size={20} className="svg-spin" /> : recording ? <Square size={18} /> : <Mic size={20} />}
        </ActionIcon>
      </Tooltip>
      {status !== "idle" ? (
        <Tooltip label="取消语音输入" openDelay={450}>
          <ActionIcon className="chat-header-square ghost" aria-label="取消语音输入" variant="subtle" onClick={cancel}><X size={18} /></ActionIcon>
        </Tooltip>
      ) : (
        <Tooltip label="音频文件转文字" openDelay={450}>
          <ActionIcon className="chat-header-square ghost" aria-label="音频文件转文字" variant="subtle" disabled={disabled} onClick={() => inputRef.current?.click()}><FileAudio size={20} /></ActionIcon>
        </Tooltip>
      )}
    </>
  );
}
