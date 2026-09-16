import { describe, expect, it } from "vitest";
describe("supplier model cascade values", () => {
  it("keeps supplier and model identifiers distinct", () => {
    expect(`supplier-1\u0000gpt-4o`.split("\u0000")).toEqual(["supplier-1", "gpt-4o"]);
  });
});
