import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { attachChatScroll } from "./chatScroll";

class Viewport extends EventTarget {
  scrollHeight = 1000;
  clientHeight = 300;
  private top = 0;
  get scrollTop() { return this.top; }
  set scrollTop(value: number) { this.top = Math.max(0, Math.min(value, this.scrollHeight - this.clientHeight)); }
  closest() { return null; }
}

describe("streamed chat scrolling", () => {
  let frames: Map<number, FrameRequestCallback>;
  let resize: () => void;
  let disconnect: ReturnType<typeof vi.fn>;
  let viewport: Viewport;
  let following: ReturnType<typeof vi.fn>;
  let scroll: ReturnType<typeof attachChatScroll>;
  const flush = () => { const pending = [...frames.values()]; frames.clear(); pending.forEach((callback) => callback(0)); };
  const emit = (type: string, properties: object) => viewport.dispatchEvent(Object.assign(new Event(type), properties));
  beforeEach(() => {
    frames = new Map();
    let id = 0;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { frames.set(++id, callback); return id; });
    vi.stubGlobal("cancelAnimationFrame", (frame: number) => frames.delete(frame));
    disconnect = vi.fn();
    vi.stubGlobal("ResizeObserver", class {
      constructor(callback: () => void) { resize = callback; }
      observe() {}
      disconnect = disconnect;
    });
    viewport = new Viewport();
    following = vi.fn();
    scroll = attachChatScroll(viewport as unknown as HTMLElement, {} as HTMLElement, following);
    flush();
  });
  afterEach(() => { scroll.dispose(); vi.unstubAllGlobals(); });

  it("starts at the latest message and follows content and viewport growth", () => {
    expect(viewport.scrollTop).toBe(700);
    viewport.scrollHeight += 200;
    resize(); flush();
    expect(viewport.scrollTop).toBe(900);
    viewport.clientHeight = 200;
    resize(); flush();
    expect(viewport.scrollTop).toBe(1000);
  });

  it("cancels an already scheduled follow as soon as the user scrolls upward", () => {
    viewport.scrollHeight += 200;
    resize();
    emit("wheel", { deltaY: -80 });
    viewport.scrollTop = 620;
    emit("scroll", {});
    flush();
    expect(viewport.scrollTop).toBe(620);
    viewport.scrollHeight += 1000;
    resize(); flush();
    expect(viewport.scrollTop).toBe(620);
    expect(following).toHaveBeenLastCalledWith(false);
  });

  it("respects scrollbar dragging and resumes only when the reader reaches the bottom", () => {
    viewport.scrollTop = 200; emit("scroll", {});
    viewport.scrollHeight += 500; resize(); flush();
    expect(viewport.scrollTop).toBe(200);
    viewport.scrollTop = 500; emit("scroll", {});
    expect(following).toHaveBeenLastCalledWith(false);
    viewport.scrollTop = viewport.scrollHeight; emit("scroll", {});
    expect(following).toHaveBeenLastCalledWith(true);
    viewport.scrollHeight += 100; resize(); flush();
    expect(viewport.scrollTop).toBe(1300);
  });

  it("supports touch and keyboard interruption and explicit return to latest", () => {
    emit("touchstart", { touches: [{ clientY: 100 }] });
    emit("touchmove", { touches: [{ clientY: 150 }] });
    viewport.scrollHeight += 100; resize(); flush();
    expect(viewport.scrollTop).toBe(700);
    scroll.jumpToLatest(); flush();
    expect(viewport.scrollTop).toBe(800);
    emit("keydown", { key: "PageUp" });
    viewport.scrollHeight += 100; resize(); flush();
    expect(viewport.scrollTop).toBe(800);
    scroll.jumpToLatest(); flush();
    expect(viewport.scrollTop).toBe(900);
  });

  it("disconnects observers and cancels pending work on conversation cleanup", () => {
    resize(); scroll.dispose(); flush();
    expect(disconnect).toHaveBeenCalled();
    viewport.scrollHeight += 500; resize(); flush();
    expect(viewport.scrollTop).toBe(700);
    emit("wheel", { deltaY: -50 });
    expect(following).toHaveBeenLastCalledWith(true);
  });
});
