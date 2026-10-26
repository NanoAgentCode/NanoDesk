import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
vi.mock("@mantine/core", () => ({ Tooltip: ({ children }: any) => children }));
import SettingsProfileTab, { createProfileSettingsDraft } from "./SettingsProfileTab";

it("explains automatic profile processing without a review queue", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("无需逐条审核");
  expect(html).toContain("长期记忆不会重复进入画像");
  expect(html).not.toContain("待确认");
  expect(html).not.toContain("加入画像");
});

it("shows only the enable switch and model configuration", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("画像模型");
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
