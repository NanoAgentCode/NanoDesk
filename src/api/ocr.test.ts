import { beforeEach, expect, it, vi } from "vitest";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { getOcrConfig, saveOcrConfig } from "./ocr";

beforeEach(() => vi.clearAllMocks());
it("loads the persisted OCR selection", async () => {
  invoke.mockResolvedValueOnce({ provider: "local" });
  expect(await getOcrConfig()).toEqual({ provider: "local" });
  expect(invoke).toHaveBeenCalledWith("get_ocr_config");
});
it.each(["local", "external", "mineru"] as const)("saves %s and propagates failures", async provider => {
  const config = { provider, external_url: "", external_key: "", model: "", mineru_url: "", mineru_key: "" };
  invoke.mockResolvedValueOnce(config);
  expect(await saveOcrConfig(config)).toEqual(config);
  expect(invoke).toHaveBeenCalledWith("save_ocr_config", { config });
  invoke.mockRejectedValueOnce(new Error("save failed"));
  await expect(saveOcrConfig(config)).rejects.toThrow("save failed");
});
