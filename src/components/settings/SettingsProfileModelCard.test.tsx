import { beforeEach, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { getProfileSettings, saveProfileSettings } from "../../api";
import { DEFAULT_DRAFT } from "../../lib/profileSettings";
import type { UseModelReturn } from "../../hooks/useModel";
vi.mock("../../api", () => ({ getProfileSettings: vi.fn(), saveProfileSettings: vi.fn() }));
vi.mock("@mantine/core", () => ({ Paper: ({ children }: any) => <section>{children}</section>, Text: ({ children }: any) => <p>{children}</p> }));
vi.mock("./SupplierModelSelectors", () => ({ SupplierModelSelect: ({ label, kind }: any) => <select aria-label={`${label}模型`} data-kind={kind} /> }));
import SettingsProfileModelCard, { saveProfileModelSelection } from "./SettingsProfileModelCard";
beforeEach(() => vi.clearAllMocks());

it.each([true, false])("saves the selected model while preserving the current enabled state (%s)", async enabled => {
  vi.mocked(getProfileSettings).mockResolvedValue({ ...DEFAULT_DRAFT, enabled, model_config_id: "old", character_threshold: 9999 } as Awaited<ReturnType<typeof getProfileSettings>>);
  await saveProfileModelSelection("new");
  expect(saveProfileSettings).toHaveBeenCalledWith({ ...DEFAULT_DRAFT, enabled, model_config_id: "new" });
});

it("does not overwrite settings when loading the current configuration fails", async () => {
  vi.mocked(getProfileSettings).mockRejectedValue(new Error("offline"));
  await expect(saveProfileModelSelection("new")).rejects.toThrow("offline");
  expect(saveProfileSettings).not.toHaveBeenCalled();
});

it("offers a chat model selector for profile extraction", () => {
  const html = renderToStaticMarkup(<SettingsProfileModelCard model={{ models: [] } as unknown as UseModelReturn} />);
  expect(html).toContain("用户画像模型");
  expect(html).toContain('aria-label="画像模型"');
  expect(html).toContain('data-kind="chat"');
});
