import { readdirSync, readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const backend = resolve("src-tauri/src");

describe("backend source layout", () => {
  it("keeps only the Rust entrypoints at the source root", () => {
    expect(readdirSync(backend).filter(name => name.endsWith(".rs")).sort()).toEqual(["lib.rs", "main.rs"]);
  });

  it("resolves explicit module paths inside the source tree", () => {
    const entry = readFileSync(resolve(backend, "lib.rs"), "utf8");
    for (const [, path] of entry.matchAll(/#\[path\s*=\s*"([^"]+)"\]/g)) {
      expect(path).not.toContain("..");
      expect(existsSync(resolve(backend, path)), path).toBe(true);
    }
  });
});
