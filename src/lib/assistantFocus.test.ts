import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAssistantBlurTimer } from "./assistantFocus";
describe("assistant loses focus", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  it("collapses only after five continuous seconds without focus", () => {
    const collapse = vi.fn(); const timer = createAssistantBlurTimer(collapse);
    timer.setFocused(false); vi.advanceTimersByTime(4999);
    expect(collapse).not.toHaveBeenCalled();
    timer.setFocused(false); vi.advanceTimersByTime(1);
    expect(collapse).toHaveBeenCalledOnce(); timer.dispose();
  });
  it("cancels on refocus and restarts on the next blur", () => {
    const collapse = vi.fn(); const timer = createAssistantBlurTimer(collapse);
    timer.setFocused(false); vi.advanceTimersByTime(4000); timer.setFocused(true);
    vi.advanceTimersByTime(5000); expect(collapse).not.toHaveBeenCalled();
    timer.setFocused(false); vi.advanceTimersByTime(5000); expect(collapse).toHaveBeenCalledOnce();
  });
  it("cancels when the input is collapsed or a file dialog opens", () => {
    const collapse = vi.fn(); const timer = createAssistantBlurTimer(collapse);
    timer.setFocused(false); timer.dispose(); vi.advanceTimersByTime(5000);
    expect(collapse).not.toHaveBeenCalled();
  });
});
