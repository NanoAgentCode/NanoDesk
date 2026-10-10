/// <reference types="vite/client" />
import backendEntrySource from "../../src-tauri/src/lib.rs?raw";
import { describe, expect, it } from "vitest";
import expected from "./ipcCommands.snapshot.json";
import expectedExports from "./apiExports.snapshot.json";

describe("desktop IPC compatibility", () => {
  it("preserves the frontend API facade exports", async () => {
    const api = await import("../api");
    expect(Object.keys(api).sort()).toEqual(expectedExports);
  });
  it("keeps every registered command name when internal modules move", () => {
    const source = backendEntrySource;
    const handler = source.match(/generate_handler!\[([\s\S]*?)\]/)?.[1];
    expect(handler).toBeDefined();
    const commands = [...handler!.matchAll(/(?:\w+::)*(\w+)\s*(?:,|$)/g)].map((match) => match[1]).sort();
    expect(commands).toEqual(expected);
    expect(new Set(commands).size).toBe(commands.length);
  });
});
