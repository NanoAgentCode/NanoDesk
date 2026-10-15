import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
vi.mock("../IconTooltipButton", () => ({ default: ({ label, children, ...props }: any) => <button {...props} aria-label={label}>{children}</button> }));
import SettingsEnvironmentTab from "./SettingsEnvironmentTab";
import type { UseEnvReturn } from "../../hooks/useEnv";
import { defaultOcrConfig } from "../../api/ocr";

it.each(["local", "external", "mineru"] as const)("shows the fields for %s", provider => {
  const env = {
    ocrConfig: { ...defaultOcrConfig, provider }, envStatus: {}, tavilyApiKey: "", envInstallProgress: "",
    isLoadingOcr: false, isSavingOcr: false
  } as UseEnvReturn;
  const html = renderToStaticMarkup(<SettingsEnvironmentTab env={env} />);
  expect(html).toContain('id="ocr-provider"');
  expect(html).toContain(`value="${provider}" selected=""`);
  expect(html.includes('id="ocr-url"')).toBe(provider !== "local");
  expect(html.includes('id="ocr-key"')).toBe(provider !== "local");
  expect(html.includes('id="ocr-model"')).toBe(provider === "external");
  expect(html).toContain("保存 OCR 配置");
  const header = html.match(/<header class="ocr-header">([\s\S]*?)<\/header>/)?.[1];
  expect(header).toBeDefined();
  expect(header).not.toContain("保存 OCR 配置");
  expect(html).toMatch(/class="ocr-provider-control">[\s\S]*?<select[^>]*id="ocr-provider"[\s\S]*?<\/select>[\s\S]*?aria-label="保存 OCR 配置"/);
  expect(html).toMatch(/<fieldset[^>]*class="ocr-fields"[^>]*>[\s\S]*?id="ocr-provider"/);
});
