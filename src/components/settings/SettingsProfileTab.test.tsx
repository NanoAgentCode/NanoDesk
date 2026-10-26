import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
vi.mock("@mantine/core", () => ({ Tooltip: ({ children }: any) => children }));
import SettingsProfileTab from "./SettingsProfileTab";

it("explains automatic profile processing without a review queue", () => {
  const html = renderToStaticMarkup(<SettingsProfileTab />);
  expect(html).toContain("无需逐条审核");
  expect(html).toContain("手工记忆不会重复进入画像");
  expect(html).not.toContain("待确认");
  expect(html).not.toContain("加入画像");
});
