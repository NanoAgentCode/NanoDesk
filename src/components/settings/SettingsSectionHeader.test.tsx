import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import SettingsSectionHeader from "./SettingsSectionHeader";

describe("SettingsSectionHeader", () => {
  it("preserves the heading, action slot and existing CSS layout", () => {
    expect(renderToStaticMarkup(<SettingsSectionHeader title="MCP 配置"><button>添加 MCP 服务器</button></SettingsSectionHeader>))
      .toBe('<div class="model-header-row"><h3>MCP 配置</h3><button>添加 MCP 服务器</button></div>');
  });
});
