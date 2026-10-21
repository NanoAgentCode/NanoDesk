import { afterEach, describe, expect, it, vi } from "vitest";
import { beginResize } from "./layoutResize";

afterEach(() => vi.unstubAllGlobals());

describe("layout drag lifecycle", () => {
  it.each(["col-resize", "row-resize"])("restores cursor and removes listeners after %s", (cursor) => {
    const target = new EventTarget();
    const classes = new Set<string>();
    const body = {
      style: { cursor: "pointer" },
      classList: { add: (name: string) => classes.add(name), remove: (name: string) => classes.delete(name) }
    };
    vi.stubGlobal("window", target);
    vi.stubGlobal("document", { body });
    const move = vi.fn();
    beginResize(move, cursor);
    expect(body.style.cursor).toBe(cursor);
    expect(classes.has("is-resizing")).toBe(true);
    const event = new Event("mousemove", { cancelable: true });
    target.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(move).toHaveBeenCalledWith(event);
    target.dispatchEvent(new Event("mouseup"));
    expect(body.style.cursor).toBe("pointer");
    expect(classes.size).toBe(0);
    target.dispatchEvent(new Event("mousemove"));
    expect(move).toHaveBeenCalledTimes(1);
  });
});
