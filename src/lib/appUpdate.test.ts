import { describe, expect, it } from "vitest";
import { selectAppUpdate, type GithubRelease } from "./appUpdate";

const release: GithubRelease = { tag_name: "v0.2.10", body: "修复问题", assets: [
  { name: "NanoDesk_0.2.10_x64-setup.exe", browser_download_url: "https://github.com/NanoAgentCode/NanoDesk/releases/download/v0.2.10/NanoDesk_0.2.10_x64-setup.exe" },
] };
describe("GitHub app updates", () => {
  it("compares numeric versions and selects the official installer", () => {
    expect(selectAppUpdate("0.2.5", release, "x64")).toEqual({ version: "v0.2.10", notes: "修复问题", downloadUrl: release.assets[0].browser_download_url });
  });
  it.each(["0.2.10", "0.3.0", "1.0.0", "invalid"])("does not offer an update for %s", current => {
    expect(selectAppUpdate(current, release, "x64")).toBeNull();
  });
  it("ignores missing releases, drafts, prereleases and incompatible installers", () => {
    for (const candidate of [null, { ...release, draft: true }, { ...release, prerelease: true }, { ...release, tag_name: "v1.0.0-beta.1" }, { ...release, assets: [] }]) {
      expect(selectAppUpdate("0.2.5", candidate, "x64")).toBeNull();
    }
    expect(selectAppUpdate("0.2.5", release, "arm64")).toBeNull();
  });
  it("rejects downloads outside the official repository", () => {
    expect(selectAppUpdate("0.2.5", { ...release, assets: [{ ...release.assets[0], browser_download_url: "https://example.com/setup.exe" }] }, "x64")).toBeNull();
  });
});
