import { describe, expect, it } from "vitest";
import { appPlugins } from "./builtin";

describe("settings contributions", () => {
  it("removes tracing while preserving usage analysis", () => {
    expect(appPlugins.findSettings("observability")).toBeUndefined();
    expect(appPlugins.settings.some((tab) => tab.label === "链路追踪")).toBe(false);
    expect(appPlugins.findSettings("usage")).toBeDefined();
  });
});
