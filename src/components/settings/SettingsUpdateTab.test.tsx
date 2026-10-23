import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import SettingsUpdateTab from "./SettingsUpdateTab";

const mocks = vi.hoisted(() => ({ version: vi.fn(), check: vi.fn(), show: vi.fn(), open: vi.fn(),
  values: [] as unknown[], setters: [] as ReturnType<typeof vi.fn>[], effects: [] as (() => (() => void))[] }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: mocks.version }));
vi.mock("../../api/updates", () => ({ checkAppUpdate: mocks.check, showAppUpdate: mocks.show, RELEASE_HISTORY_URL: "https://github.com/NanoAgentCode/NanoDesk/releases" }));
vi.mock("../../api/desktop", () => ({ openExternalUrl: mocks.open }));
vi.mock("react", async original => {
  const react = await original<typeof import("react")>();
  return { ...react,
    useState: (initial: unknown) => {
      // Mantine uses its own hooks during server rendering; leave those untouched.
      if (rendering) return react.useState(initial);
      const index = mocks.setters.length;
      const setter = vi.fn(); mocks.setters.push(setter);
      return [mocks.values[index] ?? initial, setter];
    },
    useRef: (value: unknown) => rendering ? react.useRef(value) : ({ current: value }),
    useEffect: (effect: () => (() => void), deps: unknown[]) => {
      if (rendering) return react.useEffect(effect, deps);
      mocks.effects.push(effect);
    },
  };
});
let rendering = false;
let cleanup: (() => void) | undefined;
function mount() {
  const tree = SettingsUpdateTab();
  cleanup = mocks.effects[0]();
  const card = tree.props.children[2];
  return { tree, check: card.props.children[4].props.children[1].props.onClick,
    history: card.props.children[2].props.children[1].props.onClick };
}
const flush = async () => { await new Promise(resolve => setTimeout(resolve, 0)); };
beforeEach(() => {
  vi.clearAllMocks(); mocks.values = []; mocks.setters = []; mocks.effects = [];
  mocks.version.mockResolvedValue("0.2.5"); mocks.check.mockResolvedValue(null); mocks.open.mockResolvedValue(undefined);
});
afterEach(() => { cleanup?.(); rendering = false; });
describe("update settings", () => {
  it("loads the installed app version without checking the network", async () => {
    mount(); await flush();
    expect(mocks.setters[0]).toHaveBeenCalledWith("0.2.5");
    expect(mocks.check).not.toHaveBeenCalled();
  });
  it("shows version, history and a manual check action", () => {
    mocks.values = ["0.2.5"];
    const { tree } = mount(); rendering = true;
    const html = renderToStaticMarkup(<MantineProvider>{tree}</MantineProvider>);
    expect(html).toContain("更新应用"); expect(html).toContain("0.2.5");
    expect(html).toContain("更新日志"); expect(html).toContain("检查更新");
  });
  it("shows the checking state and disables the check button", () => {
    mocks.values = ["0.2.5", true];
    const { tree } = mount(); rendering = true;
    const html = renderToStaticMarkup(<MantineProvider>{tree}</MantineProvider>);
    expect(html).toContain("正在检查…"); expect(html).toContain("disabled");
  });
  it("reports no available update", async () => {
    mount().check(); await flush();
    expect(mocks.setters[2]).toHaveBeenCalledWith("未发现可用更新。");
    expect(mocks.show).not.toHaveBeenCalled();
    expect(mocks.setters[1]).toHaveBeenLastCalledWith(false);
  });
  it("reopens the common update prompt when a new version exists", async () => {
    const update = { version: "v0.3.0", notes: "修复", downloadUrl: "https://github.com/NanoAgentCode/NanoDesk/releases/download/v0.3.0/setup.exe" };
    mocks.check.mockResolvedValue(update);
    mount().check(); await flush(); expect(mocks.show).toHaveBeenCalledWith(update);
  });
  it("reports failures and allows retry", async () => {
    mocks.check.mockRejectedValueOnce(new Error("offline"));
    const { check } = mount(); check(); await flush();
    expect(mocks.setters[3]).toHaveBeenCalledWith(expect.stringContaining("检查更新失败"));
    check(); await flush(); expect(mocks.check).toHaveBeenCalledTimes(2);
  });
  it("prevents duplicate checks while pending", async () => {
    mocks.check.mockReturnValue(new Promise(() => {}));
    const { check } = mount(); check(); check();
    expect(mocks.check).toHaveBeenCalledTimes(1);
  });
  it("opens official release history and reports browser errors", async () => {
    mocks.open.mockRejectedValueOnce(new Error("no browser"));
    mount().history(); await flush();
    expect(mocks.open).toHaveBeenCalledWith("https://github.com/NanoAgentCode/NanoDesk/releases");
    expect(mocks.setters[3]).toHaveBeenCalledWith("无法打开更新日志，请重试。");
  });
  it("ignores completed checks after leaving the page", async () => {
    const { check } = mount(); check(); cleanup?.(); await flush();
    expect(mocks.show).not.toHaveBeenCalled();
    expect(mocks.setters[2]).not.toHaveBeenCalledWith("未发现可用更新。");
  });
});
