import { describe, expect, it } from "vitest";

import type { Memory } from "../types";
import { mergeCreatedMemory } from "./useMemory";

function memory(id: string, title = id): Memory {
  return {
    id,
    title,
    content: `${title} content`,
    tags: ["chat"],
    enabled: true,
    created_at: "2026-08-27T00:00:00Z",
    updated_at: "2026-08-27T00:00:00Z"
  };
}

describe("mergeCreatedMemory", () => {
  it("shows a newly created memory immediately", () => {
    const created = memory("new", "New memory");

    expect(mergeCreatedMemory([], created)).toEqual([created]);
  });

  it("keeps a created memory unique and moves the latest value first", () => {
    const oldValue = memory("same", "Old value");
    const other = memory("other", "Other memory");
    const created = memory("same", "Latest value");

    expect(mergeCreatedMemory([other, oldValue], created)).toEqual([created, other]);
  });
});
