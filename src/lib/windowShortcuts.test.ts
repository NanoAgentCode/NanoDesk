import { describe, expect, it, vi } from "vitest";
import { registerWindowShortcuts } from "./windowShortcuts";

function keyDown(key: string, properties = {}) {
  return Object.assign(new Event("keydown", { cancelable: true }), {
    key, repeat: false, isComposing: false,
    ctrlKey: false, altKey: false, shiftKey: false, metaKey: false,
    ...properties
  });
}

describe("window shortcuts", () => {
  it("closes an open page first and minimizes on the next Escape", () => {
    const target = new EventTarget();
    const minimize = vi.fn().mockResolvedValue(undefined);
    let dialogMounted = true;
    const closePage = vi.fn();
    const cleanup = registerWindowShortcuts(target, minimize, vi.fn(), () => dialogMounted);
    target.addEventListener("keydown", closePage);

    const firstEscape = keyDown("Escape");
    target.dispatchEvent(firstEscape);
    expect(closePage).toHaveBeenCalledTimes(1);
    expect(firstEscape.defaultPrevented).toBe(false);
    expect(minimize).not.toHaveBeenCalled();

    dialogMounted = false;
    target.removeEventListener("keydown", closePage);
    target.dispatchEvent(keyDown("Escape"));
    expect(minimize).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("minimizes on Escape and removes the listener on cleanup", () => {
    const target = new EventTarget();
    const minimize = vi.fn().mockResolvedValue(undefined);
    const cleanup = registerWindowShortcuts(target, minimize, vi.fn());
    const event = keyDown("Escape");
    target.dispatchEvent(event);
    expect(minimize).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
    cleanup();
    target.dispatchEvent(keyDown("Escape"));
    expect(minimize).toHaveBeenCalledTimes(1);
  });

  it("ignores other keys, modified Escape, repeats and composition", () => {
    const target = new EventTarget();
    const minimize = vi.fn().mockResolvedValue(undefined);
    const cleanup = registerWindowShortcuts(target, minimize, vi.fn());
    target.dispatchEvent(keyDown("Enter"));
    for (const property of ["repeat", "isComposing", "ctrlKey", "altKey", "shiftKey", "metaKey"]) {
      target.dispatchEvent(keyDown("Escape", { [property]: true }));
    }
    expect(minimize).not.toHaveBeenCalled();
    cleanup();
  });

  it("reports a window minimize failure", async () => {
    const target = new EventTarget();
    const notice = vi.fn();
    const cleanup = registerWindowShortcuts(target, vi.fn().mockRejectedValue(new Error("denied")), notice);
    target.dispatchEvent(keyDown("Escape"));
    await Promise.resolve();
    expect(notice).toHaveBeenCalledWith("最小化窗口失败：Error: denied");
    cleanup();
  });
});
