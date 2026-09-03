import { describe, expect, it } from "vitest";
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampSidebarWidth,
  parseSidebarWidth
} from "./sidebarSizing";

describe("sidebar sizing", () => {
  it("keeps the width within the supported resize range", () => {
    expect(clampSidebarWidth(180)).toBe(SIDEBAR_MIN_WIDTH);
    expect(clampSidebarWidth(320)).toBe(320);
    expect(clampSidebarWidth(520)).toBe(SIDEBAR_MAX_WIDTH);
  });

  it("restores a valid saved width and falls back for invalid values", () => {
    expect(parseSidebarWidth("360")).toBe(360);
    expect(parseSidebarWidth("999")).toBe(SIDEBAR_MAX_WIDTH);
    expect(parseSidebarWidth(null)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(parseSidebarWidth("invalid")).toBe(SIDEBAR_DEFAULT_WIDTH);
  });
});
