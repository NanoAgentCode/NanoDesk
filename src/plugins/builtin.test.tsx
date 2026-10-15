import { describe, expect, it, vi } from "vitest";
import { Suspense } from "react";
import { renderToPipeableStream } from "react-dom/server";
import { Writable } from "node:stream";

const loaded = vi.hoisted(() => ({ ops: 0, automation: 0, usage: 0 }));
vi.mock("../components/OpsPanel", () => {
  loaded.ops += 1;
  return { default: () => <div>Operations loaded</div> };
});
vi.mock("../components/AutomationPanel", () => {
  loaded.automation += 1;
  return { default: () => <div>Automation loaded</div> };
});
vi.mock("../components/settings/SettingsUsageTab", () => {
  loaded.usage += 1;
  return { default: () => <div>Usage loaded</div> };
});
import { appPlugins } from "./builtin";

describe("settings contributions", () => {
  it("removes tracing while preserving usage analysis", () => {
    expect(appPlugins.findSettings("observability")).toBeUndefined();
    expect(appPlugins.settings.some((tab) => tab.label === "链路追踪")).toBe(false);
    expect(appPlugins.findSettings("usage")).toBeDefined();
  });
});


describe("page loading", () => {
  it("loads only the selected page, leaving unrelated pages deferred", async () => {
    expect(loaded).toEqual({ ops: 0, automation: 0, usage: 0 });
    const Page = appPlugins.findMainView("ops")!.component;
    const html = await new Promise<string>((resolve, reject) => {
      let output = "";
      const destination = new Writable({
        write(chunk, _encoding, done) { output += chunk.toString(); done(); }
      });
      destination.on("finish", () => resolve(output));
      destination.on("error", reject);
      const stream = renderToPipeableStream(
        <Suspense fallback={<div>Loading</div>}><Page setNotice={() => {}} /></Suspense>,
        {
          onAllReady() { stream.pipe(destination); },
          onError: reject
        }
      );
    });
    expect(html).toContain("Operations loaded");
    expect(loaded).toEqual({ ops: 1, automation: 0, usage: 0 });
  });
});
