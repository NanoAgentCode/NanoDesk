import { describe, expect, it, vi } from "vitest";
import { SpeechRecorder } from "./speechRecorder";

function setup() {
  const stopTrack = vi.fn();
  const stream = { getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream;
  const recorder = {
    state: "inactive",
    mimeType: "audio/webm",
    ondataavailable: null as ((event: { data: Blob }) => void) | null,
    onstop: null as (() => void) | null,
    onerror: null as (() => void) | null,
    start: vi.fn(function (this: { state: string }) { this.state = "recording"; }),
    stop: vi.fn(function (this: { state: string }) { this.state = "inactive"; })
  };
  const onError = vi.fn();
  const session = new SpeechRecorder(stream, recorder as unknown as MediaRecorder, onError);
  return { session, recorder, stopTrack, onError };
}

describe("microphone lifecycle", () => {
  it("collects final audio and releases microphone on stop", async () => {
    const { session, recorder, stopTrack } = setup();
    recorder.ondataavailable?.({ data: new Blob(["first-"]) });
    const result = session.stop();
    expect(stopTrack).toHaveBeenCalled();
    recorder.ondataavailable?.({ data: new Blob(["last"]) });
    recorder.onstop?.();
    expect(await (await result).text()).toBe("first-last");
  });

  it("cancels pending audio and ignores late recording data", async () => {
    const { session, recorder, stopTrack } = setup();
    const result = session.stop();
    session.cancel();
    recorder.ondataavailable?.({ data: new Blob(["discard"]) });
    recorder.onstop?.();
    await expect(result).rejects.toThrow("取消");
    expect(stopTrack).toHaveBeenCalled();
  });

  it("releases microphone and reports recorder errors", () => {
    const { recorder, stopTrack, onError } = setup();
    recorder.onerror?.();
    expect(stopTrack).toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith(expect.any(Error));
  });
});
