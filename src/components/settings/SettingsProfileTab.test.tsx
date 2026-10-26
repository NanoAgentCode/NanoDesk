import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
vi.mock("@mantine/core", () => ({ Tooltip: ({ children }: any) => children }));
import SettingsProfileTab, { saveProfileEnabled } from "./SettingsProfileTab";
import { createProfileSettingsDraft } from "../../lib/profileSettings";

it("explains automatic profile processing without a review queue", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("无需逐条审核");
  expect(html).toContain("长期记忆不会重复进入画像");
  expect(html).not.toContain("待确认");
  expect(html).not.toContain("加入画像");
});

it("keeps the enable switch and directs model configuration to routing", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("画像模型在“模型路由”中配置");
  expect(html).not.toContain("<select");
  expect(html.match(/type="checkbox"/g)).toHaveLength(1);
  expect(html).not.toContain('type="number"');
  expect(html).not.toContain("展开配置");
});

it("uses default scheduling and budgets instead of previously customized values", () => {
  const draft = createProfileSettingsDraft({ enabled: true, model_config_id: "profile-model", character_threshold: 9999 } as Parameters<typeof createProfileSettingsDraft>[0]);
  expect(draft).toEqual({
    enabled: true, model_config_id: "profile-model", character_threshold: 3000,
    idle_seconds: 1800, max_wait_seconds: 86400, long_input_threshold: 8000,
    rolling_hour_attempt_limit: 2, rolling_day_attempt_limit: 8,
    rolling_day_candidate_character_limit: 30000
  });
});

it("integrates configuration and profile content in one card", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  const sections = html.match(/<section\b[\s\S]*?<\/section>/g) || [];
  expect(sections).toHaveLength(1);
  expect(sections[0]).toContain('id="user-profile-title"');
  expect(sections[0]).toContain('aria-label="画像配置"');
  expect(sections[0]).toContain("画像模型");
  expect(sections[0]).not.toContain("保存设置");
  expect(sections[0]).toContain("正在读取画像");
});

it("groups profile operations and distinguishes generation from worker wake-up", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain('role="group" aria-label="画像操作"');
  expect(html).toContain('aria-label="立即生成用户画像"');
  expect(html).not.toContain("<span>立即生成</span>");
  expect(html).toContain('aria-label="立即唤醒"');
  expect(html).not.toContain("<span>立即唤醒</span>");
});

vi.mock("../../api", async importOriginal => ({
  ...await importOriginal<typeof import("../../api")>(),
  getProfileSettings: vi.fn(), saveProfileSettings: vi.fn()
}));
import { getProfileSettings, saveProfileSettings } from "../../api";
import { DEFAULT_DRAFT } from "../../lib/profileSettings";

it.each([true, false])("automatically persists the switch value (%s) with the latest model", async enabled => {
  vi.mocked(getProfileSettings).mockResolvedValue({ ...DEFAULT_DRAFT, enabled: !enabled, model_config_id: "latest-model" } as Awaited<ReturnType<typeof getProfileSettings>>);
  vi.mocked(saveProfileSettings).mockResolvedValue({ ...DEFAULT_DRAFT, enabled, model_config_id: "latest-model" } as Awaited<ReturnType<typeof saveProfileSettings>>);
  expect((await saveProfileEnabled(enabled)).enabled).toBe(enabled);
  expect(saveProfileSettings).toHaveBeenLastCalledWith({ ...DEFAULT_DRAFT, enabled, model_config_id: "latest-model" });
});

it("propagates save errors so the switch keeps its last saved state", async () => {
  vi.mocked(saveProfileSettings).mockRejectedValueOnce(new Error("save failed"));
  await expect(saveProfileEnabled(true)).rejects.toThrow("save failed");
});

it("uses a status icon beside the switch instead of a text label", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("lucide-power-off");
  expect(html).toContain('role="switch"');
  expect(html).not.toContain("<span>已关闭</span>");
  expect(html).not.toContain("<span>已启用</span>");
});
