import { useEffect, useRef, useState } from "react";
import { getAsrConfig, transcribeAudio, transcribeAudioFile } from "../api";
import { fileToDataUrl } from "../lib/imageAttachments";
import { recordingToWav, validateAudioFile } from "../lib/speech";
import { attachmentName, type AttachmentSource } from "../lib/attachmentUploads";
import { SpeechRecorder, startSpeechRecording } from "../lib/speechRecorder";

export interface SpeechBatchResult { count: number; errors: string[]; cancelled: boolean }
export interface UseSpeechInputReturn {
  status: "idle" | "starting" | "recording" | "transcribing";
  seconds: number;
  isBusy: () => boolean;
  cancel: () => void;
  beginRecording: () => Promise<void>;
  finishRecording: () => Promise<void>;
  transcribeSources: (sources: AttachmentSource[]) => Promise<SpeechBatchResult>;
}

export function useSpeechInput(args: {
  scopeKey: string;
  disabled: boolean;
  onTranscript: (text: string) => void;
  setNotice: (message: string) => void;
}): UseSpeechInputReturn {
  const [status, setStatus] = useState<UseSpeechInputReturn["status"]>("idle");
  const [seconds, setSeconds] = useState(0);
  const recorderRef = useRef<SpeechRecorder | null>(null);
  const operation = useRef(0);
  const operationScope = useRef(args.scopeKey);
  const locked = useRef(false);
  const cancelBatchRef = useRef<(() => void) | null>(null);
  const latest = useRef(args);
  latest.current = args;

  function release() {
    operation.current++;
    recorderRef.current?.cancel();
    recorderRef.current = null;
    cancelBatchRef.current?.();
    cancelBatchRef.current = null;
    locked.current = false;
  }
  function cancel() { release(); setStatus("idle"); }

  useEffect(() => { cancel(); return release; }, [args.scopeKey, args.disabled]);
  useEffect(() => {
    if (status !== "recording") return;
    const timer = window.setInterval(() => setSeconds((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [status]);
  useEffect(() => { if (status === "recording" && seconds >= 300) void finishRecording(); }, [seconds, status]);

  const valid = (id: number) => id === operation.current && !latest.current.disabled && latest.current.scopeKey === operationScope.current;
  function finish(id: number) {
    if (!valid(id)) return;
    locked.current = false;
    cancelBatchRef.current = null;
    setStatus("idle");
  }
  function fail(error: unknown, id: number) {
    if (!valid(id)) return;
    const denied = error instanceof DOMException && error.name === "NotAllowedError";
    latest.current.setNotice(denied ? "麦克风权限被拒绝，请在系统隐私设置中允许访问麦克风" : `语音识别失败：${String(error)}`);
    cancel();
  }

  async function readTranscript(source: AttachmentSource, id: number) {
    if (source.kind === "path") return transcribeAudioFile(source.path);
    const error = validateAudioFile(source.file);
    if (error) throw new Error(error);
    const data = await fileToDataUrl(source.file);
    if (!valid(id)) return null;
    return transcribeAudio(source.file.name, data.slice(data.indexOf(",") + 1));
  }

  async function transcribeSources(sources: AttachmentSource[]): Promise<SpeechBatchResult> {
    if (!sources.length) return { count: 0, errors: [], cancelled: false };
    if (locked.current || latest.current.disabled) return { count: 0, errors: ["当前暂不能进行语音识别"], cancelled: true };
    locked.current = true;
    operationScope.current = latest.current.scopeKey;
    const id = ++operation.current;
    setStatus("transcribing");
    const result: SpeechBatchResult = { count: 0, errors: [], cancelled: false };
    const cancelled = new Promise<SpeechBatchResult>((resolve) => {
      cancelBatchRef.current = () => resolve({ ...result, cancelled: true });
    });
    const worker = async () => {
      for (const source of sources) {
        if (!valid(id)) { result.cancelled = true; break; }
        try {
          const text = await readTranscript(source, id);
          if (!valid(id)) { result.cancelled = true; break; }
          if (text) { latest.current.onTranscript(text); result.count++; }
        } catch (error) {
          if (!valid(id)) { result.cancelled = true; break; }
          result.errors.push(`${attachmentName(source)}：${String(error)}`);
        }
      }
      return result;
    };
    try { return await Promise.race([worker(), cancelled]); }
    finally { finish(id); }
  }

  async function beginRecording() {
    if (locked.current || latest.current.disabled) return;
    locked.current = true;
    operationScope.current = latest.current.scopeKey;
    const id = ++operation.current;
    setStatus("starting");
    try {
      if (!(await getAsrConfig())) throw new Error("请先在系统设置 → 模型路由中选择语音识别模型");
      if (!valid(id)) return;
      const recorder = await startSpeechRecording((error) => fail(error, id));
      if (!valid(id)) { recorder.cancel(); return; }
      recorderRef.current = recorder;
      setSeconds(0); setStatus("recording");
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
      const text = await readTranscript({ kind: "file", file: await recordingToWav(blob) }, id);
      if (!valid(id)) return;
      if (text) latest.current.onTranscript(text);
      latest.current.setNotice("语音已转为文字，请检查后发送");
      finish(id);
    } catch (error) { fail(error, id); }
  }

  return { status, seconds, isBusy: () => locked.current, cancel, beginRecording, finishRecording, transcribeSources };
}
