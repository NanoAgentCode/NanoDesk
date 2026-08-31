import { describe, expect, it } from "vitest";
import { ACCESS_MODE_OPTIONS, getAccessModeOption, parseAccessMode } from "./accessMode";

describe("agent access modes", () => {
  it("falls back to the safest mode for missing or unknown values", () => {
    expect(parseAccessMode(null)).toBe("ask");
    expect(parseAccessMode("unknown")).toBe("ask");
  });

  it("preserves supported persisted values", () => {
    expect(parseAccessMode("auto")).toBe("auto");
    expect(parseAccessMode("full")).toBe("full");
  });

  it("defines all three user-facing modes", () => {
    expect(ACCESS_MODE_OPTIONS.map((option) => option.value)).toEqual(["ask", "auto", "full"]);
    expect(getAccessModeOption("full").label).toBe("完全访问");
  });
});
