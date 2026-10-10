import { describe, expect, it } from "vitest";
import { parseOnceTime, triggerLabel } from "./automation";

describe("automation scheduling display", () => {
  it("shows a stored timezone independent of the computer timezone", () => {
    expect(triggerLabel({ kind: "daily", hour: 9, minute: 5, utc_offset_minutes: 480 })).toBe(
      "每日 09:05 · UTC+08:00"
    );
    expect(triggerLabel({ kind: "daily", hour: 9, minute: 5, utc_offset_minutes: -210 })).toContain(
      "UTC-03:30"
    );
  });
  it("rejects invalid once times and converts timestamps to seconds", () => {
    expect(() => parseOnceTime("")).toThrow();
    expect(parseOnceTime("2026-10-10T09:00:00+08:00")).toBe(Date.parse("2026-10-10T01:00:00Z") / 1000);
  });
});
