import { describe, expect, it } from "vitest";
import { mergeRecoveredProjects } from "./projects";

describe("project recovery", () => {
  it("recovers database-backed projects without duplicating saved entries", () => {
    const saved = [{ id: "D:/one", name: "One", path: "D:/one", opened_at: "old" }];
    expect(mergeRecoveredProjects(saved, ["d:/ONE", "D:/two", "D:/two"], "now")).toEqual([
      ...saved,
      { id: "D:/two", name: "two", path: "D:/two", opened_at: "now" }
    ]);
  });
});
