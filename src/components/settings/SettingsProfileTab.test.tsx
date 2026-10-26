import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
vi.mock("@mantine/core", () => ({ Tooltip: ({ children }: any) => children }));
import SettingsProfileTab from "./SettingsProfileTab";
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
  expect(sections[0]).toContain("保存设置");
  expect(sections[0]).toContain("尚未形成画像");
});

it("groups profile operations and distinguishes generation from worker wake-up", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain('role="group" aria-label="画像操作"');
  expect(html).toContain("<span>立即生成</span>");
  expect(html).toContain("<span>立即唤醒</span>");
});
