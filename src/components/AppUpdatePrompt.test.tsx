import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import AppUpdatePrompt from "./AppUpdatePrompt";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn(), update: null as unknown, setters: [] as ReturnType<typeof vi.fn>[], effects: [] as (() => (() => void))[] }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../api/desktop", () => ({ openExternalUrl: mocks.open }));
vi.mock("react", async original => ({ ...await original<typeof import("react")>(),
  useState: (value: unknown) => { const setter = vi.fn(); mocks.setters.push(setter); return [mocks.setters.length === 1 ? mocks.update : value, setter]; },
  useEffect: (effect: () => (() => void)) => { mocks.effects.push(effect); },
}));
const release = { tag_name: "v0.3.0", body: "更新说明", assets: [{ name: "NanoDesk_0.3.0_x64-setup.exe", browser_download_url: "https://github.com/NanoAgentCode/NanoDesk/releases/download/v0.3.0/setup.exe" }] };
let cleanup: (() => void) | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("window", globalThis);
  mocks.setters.length = 0; mocks.effects.length = 0;
  mocks.update = null; mocks.open.mockReset().mockResolvedValue(undefined);
  mocks.invoke.mockReset().mockResolvedValue({ currentVersion: "0.2.5", arch: "x64", release });
  AppUpdatePrompt();
  cleanup = mocks.effects[0]();
});
afterEach(() => { cleanup?.(); vi.useRealTimers(); vi.unstubAllGlobals(); });
describe("automatic update scheduling", () => {
  it("checks after startup and suppresses repeat prompts for the same version", async () => {
    expect(mocks.invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(5000);
    expect(mocks.invoke).toHaveBeenCalledWith("check_app_update");
    expect(mocks.setters[0]).toHaveBeenCalledWith(expect.objectContaining({ version: "v0.3.0" }));
    await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000);
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    expect(mocks.setters[0]).toHaveBeenCalledTimes(1);
  });
  it("retries a failed check without showing an error prompt", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("offline"));
    await vi.advanceTimersByTimeAsync(5000);
    expect(mocks.setters[0]).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000);
    expect(mocks.setters[0]).toHaveBeenCalledTimes(1);
  });
  it("cleans up checks when the main window unmounts", async () => {
    cleanup?.();
    await vi.advanceTimersByTimeAsync(7 * 60 * 60 * 1000);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
  it("ignores responses arriving after cleanup", async () => {
    let resolve!: (value: unknown) => void;
    mocks.invoke.mockReturnValueOnce(new Promise(done => { resolve = done; }));
    await vi.advanceTimersByTimeAsync(5000);
    cleanup?.();
    resolve({ currentVersion: "0.2.5", arch: "x64", release });
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.setters[0]).not.toHaveBeenCalled();
  });
  it("opens the selected GitHub download and allows retry after failure", async () => {
    mocks.setters.length = 0;
    mocks.update = { version: "v0.3.0", notes: "更新说明", downloadUrl: release.assets[0].browser_download_url };
    const modal = AppUpdatePrompt();
    const buttons = modal.props.children.props.children[4].props.children;
    mocks.open.mockRejectedValueOnce(new Error("browser unavailable"));
    buttons[1].props.onClick();
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.open).toHaveBeenCalledWith(release.assets[0].browser_download_url);
    expect(mocks.setters[1]).toHaveBeenCalledWith("无法打开下载链接，请重试。");
    expect(mocks.setters[2]).toHaveBeenLastCalledWith(false);
    buttons[1].props.onClick();
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.open).toHaveBeenCalledTimes(2);
    buttons[0].props.onClick();
    expect(mocks.setters[0]).toHaveBeenCalledWith(null);
  });
});
