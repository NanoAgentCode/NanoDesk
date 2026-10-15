import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ASSISTANT_TRANSITION_MS, createAssistantTransition } from "./assistantTransition";
import { createAssistantBlurTimer } from "./assistantFocus";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function setup() {
  const resize = vi.fn().mockResolvedValue(undefined);
  const phase = vi.fn();
  const error = vi.fn();
  return { resize, phase, error, transition: createAssistantTransition(resize, phase, error) };
}

it("plays the closing animation after the five-second automatic blur timeout", async () => {
  const { resize, phase, transition } = setup();
  transition.update("input");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  const blur = createAssistantBlurTimer(() => transition.update("icon"));
  blur.setFocused(false);
  await vi.advanceTimersByTimeAsync(4999);
  expect(phase).toHaveBeenLastCalledWith("open");
  await vi.advanceTimersByTimeAsync(1);
  expect(phase).toHaveBeenLastCalledWith("closing");
  expect(resize).not.toHaveBeenCalledWith("icon");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  expect(phase).toHaveBeenLastCalledWith("icon");
  blur.dispose(); transition.dispose();
});

it("waits for the larger native window before playing the opening animation", async () => {
  const { resize, phase, transition } = setup();
  let complete!: () => void;
  resize.mockImplementationOnce(() => new Promise<void>(resolve => { complete = resolve; }));
  transition.update("input");
  await vi.advanceTimersByTimeAsync(0);
  expect(phase).toHaveBeenLastCalledWith("preparing");
  complete();
  await vi.advanceTimersByTimeAsync(0);
  expect(phase).toHaveBeenLastCalledWith("opening");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  expect(phase).toHaveBeenLastCalledWith("open");
  transition.dispose();
});

it("keeps the panel until the closing animation finishes, then shrinks the window", async () => {
  const { resize, phase, transition } = setup();
  transition.update("input");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  transition.update("icon");
  expect(phase).toHaveBeenLastCalledWith("closing");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS - 1);
  expect(resize).not.toHaveBeenCalledWith("icon");
  await vi.advanceTimersByTimeAsync(1);
  expect(resize).toHaveBeenLastCalledWith("icon");
  expect(phase).toHaveBeenLastCalledWith("icon");
  transition.dispose();
});

it("cancels a pending collapse when reopened and does not replay animation for the picker", async () => {
  const { resize, phase, transition } = setup();
  transition.update("input");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  transition.update("icon");
  await vi.advanceTimersByTimeAsync(100);
  transition.update("input");
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  expect(resize).not.toHaveBeenCalledWith("icon");
  phase.mockClear();
  transition.update("picker");
  await vi.advanceTimersByTimeAsync(0);
  expect(resize).toHaveBeenLastCalledWith("picker");
  expect(phase).not.toHaveBeenCalled();
  transition.dispose();
});

it("serializes an unfinished expansion and a newer collapse", async () => {
  const { resize, phase, transition } = setup();
  let complete!: () => void;
  resize.mockImplementationOnce(() => new Promise<void>(resolve => { complete = resolve; }));
  transition.update("input");
  await vi.advanceTimersByTimeAsync(0);
  transition.update("icon");
  expect(resize).toHaveBeenCalledTimes(1);
  complete();
  await vi.advanceTimersByTimeAsync(ASSISTANT_TRANSITION_MS);
  expect(resize).toHaveBeenLastCalledWith("icon");
  expect(phase).toHaveBeenLastCalledWith("icon");
  expect(phase).not.toHaveBeenCalledWith("opening");
  transition.dispose();
});

it("cleans timers on disposal and reports native resize errors", async () => {
  const { resize, phase, error, transition } = setup();
  resize.mockRejectedValueOnce(new Error("resize failed"));
  transition.update("input");
  await vi.advanceTimersByTimeAsync(0);
  expect(error).toHaveBeenCalledWith(expect.any(Error), true);
  expect(phase).toHaveBeenLastCalledWith("icon");
  transition.update("input");
  await vi.advanceTimersByTimeAsync(0);
  transition.dispose();
  phase.mockClear();
  await vi.advanceTimersByTimeAsync(1000);
  expect(phase).not.toHaveBeenCalled();
});
