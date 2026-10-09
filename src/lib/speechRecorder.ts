/** Owns microphone tracks; cancel never returns audio for transcription. */
export class SpeechRecorder {
  private chunks: Blob[] = [];
  private resolve: ((blob: Blob) => void) | null = null;
  private reject: ((error: Error) => void) | null = null;
  private cancelled = false;

  constructor(private stream: MediaStream, private recorder: MediaRecorder, onError: (error: Error) => void = () => {}) {
    recorder.ondataavailable = (event) => {
      if (!this.cancelled && event.data.size) this.chunks.push(event.data);
    };
    recorder.onstop = () => {
      this.release();
      if (!this.cancelled) this.resolve?.(new Blob(this.chunks, { type: recorder.mimeType }));
      this.chunks = [];
    };
    recorder.onerror = () => {
      const error = new Error("录音失败，请检查麦克风后重试");
      this.reject?.(error);
      this.cancel();
      onError(error);
    };
    try { recorder.start(1000); }
    catch (error) { this.release(); throw error; }
  }

  stop(): Promise<Blob> {
    return new Promise((resolve, reject) => {
      if (this.cancelled || this.recorder.state !== "recording") {
        reject(new Error("录音已结束，请重新录音"));
        return;
      }
      this.resolve = resolve;
      this.reject = reject;
      this.recorder.stop();
      this.release();
    });
  }

  cancel() {
    this.cancelled = true;
    this.reject?.(new Error("录音已取消"));
    this.resolve = null;
    this.reject = null;
    if (this.recorder.state !== "inactive") this.recorder.stop();
    this.release();
    this.chunks = [];
  }

  private release() { this.stream.getTracks().forEach((track) => track.stop()); }
}

export async function startSpeechRecording(onError?: (error: Error) => void): Promise<SpeechRecorder> {
  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
    throw new Error("当前环境不支持麦克风录音，请使用音频文件");
  }
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  try { return new SpeechRecorder(stream, new MediaRecorder(stream), onError); }
  catch (error) { stream.getTracks().forEach((track) => track.stop()); throw error; }
}
