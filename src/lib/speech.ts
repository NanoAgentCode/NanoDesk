import type { AsrConfig } from "../types";

export const MAX_AUDIO_BYTES = 25 * 1024 * 1024;
export const AUDIO_ACCEPT = ".mp3,.mp4,.mpeg,.mpga,.m4a,.wav,.webm";
export function isSupportedAudioAttachment(name: string): boolean {
  return /\.(mp3|mp4|mpeg|mpga|m4a|wav|webm)$/i.test(name);
}
export function validateAsrConfig(config: AsrConfig): string | null {
  if (!config.base_url.trim()) return "请填写 ASR 服务地址";
  try {
    const url = new URL(config.base_url.trim());
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
      return "服务地址必须是无账号、查询参数和片段的 HTTP/HTTPS URL";
    }
  } catch { return "请填写有效的 HTTP/HTTPS 服务地址"; }
  if (!config.model.trim()) return "请填写 ASR 模型名称";
  if (config.language.trim() && !/^[a-z]{2}$/i.test(config.language.trim())) return "语言请填写两位代码，例如 zh、en，或留空";
  return null;
}

export function validateAudioFile(file: { name: string; size: number }): string | null {
  if (!isSupportedAudioAttachment(file.name)) return "不支持的音频格式，请使用 WAV、MP3、M4A、MP4、MPEG、MPGA 或 WebM";
  if (file.size === 0) return "音频文件为空";
  if (file.size > MAX_AUDIO_BYTES) return "音频文件不能超过 25 MB";
  return null;
}

export function appendTranscript(current: string, transcript: string): string {
  const text = transcript.trim();
  if (!text) return current;
  return current + (current && !/\s$/.test(current) ? "\n" : "") + text;
}

export function encodeMonoWav(channels: Float32Array[], sampleRate: number): ArrayBuffer {
  const length = channels[0].length;
  const buffer = new ArrayBuffer(44 + length * 2);
  const view = new DataView(buffer);
  const write = (offset: number, text: string) => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };
  write(0, "RIFF");
  view.setUint32(4, 36 + length * 2, true);
  write(8, "WAVE");
  write(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  write(36, "data");
  view.setUint32(40, length * 2, true);
  for (let i = 0; i < length; i++) {
    const sample = Math.max(-1, Math.min(1, channels.reduce((sum, channel) => sum + channel[i], 0) / channels.length));
    view.setInt16(44 + i * 2, sample < 0 ? sample * 32768 : sample * 32767, true);
  }
  return buffer;
}

export async function recordingToWav(blob: Blob): Promise<File> {
  const context = new AudioContext({ sampleRate: 16000 });
  try {
    const audio = await context.decodeAudioData(await blob.arrayBuffer());
    const channels = Array.from({ length: audio.numberOfChannels }, (_, index) => audio.getChannelData(index));
    return new File([encodeMonoWav(channels, audio.sampleRate)], "recording.wav", { type: "audio/wav" });
  } finally { await context.close(); }
}
