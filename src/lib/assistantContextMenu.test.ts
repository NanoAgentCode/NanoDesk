import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ create: vi.fn(), popup: vi.fn(), close: vi.fn(), setVisible: vi.fn() }));
vi.mock("@tauri-apps/api/menu", () => ({ Menu: { new: mocks.create } }));
vi.mock("../api/assistant", () => ({ setAssistantVisible: mocks.setVisible }));
import { registerAssistantContextMenu } from "./assistantContextMenu";

describe("assistant context menu", () => {
  beforeEach(() => vi.clearAllMocks());
  it("suppresses the default menu, offers only close, and turns the assistant off", async () => {
    mocks.create.mockResolvedValue({ popup: mocks.popup, close: mocks.close });
    mocks.setVisible.mockResolvedValue(undefined);
    const surface = new EventTarget();
    const onError = vi.fn();
    const dispose = registerAssistantContextMenu(surface, onError);
    const event = new Event("contextmenu", { cancelable: true });
    surface.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    await vi.waitFor(() => expect(mocks.popup).toHaveBeenCalledOnce());
    const items = mocks.create.mock.calls[0][0].items;
    expect(items).toHaveLength(1);
    expect(items[0].text).toBe("关闭");
    items[0].action();
    await vi.waitFor(() => expect(mocks.setVisible).toHaveBeenCalledWith(false));
    dispose();
    await vi.waitFor(() => expect(mocks.close).toHaveBeenCalledOnce());
    const later = new Event("contextmenu", { cancelable: true });
    surface.dispatchEvent(later);
    expect(later.defaultPrevented).toBe(false);
    expect(onError).not.toHaveBeenCalled();
  });
  it("still suppresses the default menu if menu creation fails", async () => {
    mocks.create.mockRejectedValueOnce(new Error("menu failed"));
    const surface = new EventTarget();
    const onError = vi.fn();
    const dispose = registerAssistantContextMenu(surface, onError);
    const event = new Event("contextmenu", { cancelable: true });
    surface.dispatchEvent(event);
    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(expect.objectContaining({ message: "menu failed" })));
    expect(event.defaultPrevented).toBe(true);
    dispose();
  });
});
