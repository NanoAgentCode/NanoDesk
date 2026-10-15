import { invoke } from "@tauri-apps/api/core";

export interface OcrConfig {
  provider: "local" | "external" | "mineru";
  external_url: string;
  external_key: string;
  model: string;
  mineru_url: string;
  mineru_key: string;
}
export const defaultOcrConfig: OcrConfig = {
  provider: "local", external_url: "", external_key: "", model: "",
  mineru_url: "http://127.0.0.1:8000/file_parse", mineru_key: ""
};
export function getOcrConfig() { return invoke<OcrConfig>("get_ocr_config"); }
export function saveOcrConfig(config: OcrConfig) { return invoke<OcrConfig>("save_ocr_config", { config }); }
