import { invoke } from "@tauri-apps/api/core";
import type { AsrConfig } from "../types";

export function getAsrConfig() {
  return invoke<AsrConfig | null>("get_asr_config");
}

export function saveAsrConfig(config: AsrConfig) {
  return invoke<AsrConfig>("save_asr_config", { config });
}

export function transcribeAudio(fileName: string, audioBase64: string, config?: AsrConfig) {
  return invoke<string>("transcribe_audio", { fileName, audioBase64, config: config ?? null });
}

export function transcribeAudioFile(path: string) {
  return invoke<string>("transcribe_audio_file", { path });
}
