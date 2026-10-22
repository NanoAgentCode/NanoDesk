import { Suspense } from "react";
import { renderToPipeableStream } from "react-dom/server";
import { Writable } from "node:stream";
import { describe, expect, it, vi } from "vitest";

const loaded = vi.hoisted(() => ({ app: 0, document: 0, assistant: 0 }));
vi.mock("./App", () => { loaded.app++; return { default: () => <div>Main window</div> }; });
vi.mock("./components/DocumentChat", () => {
  loaded.document++;
  return { default: ({ path }: { path: string }) => <div>Document: {path}</div> };
});
vi.mock("./components/AssistantMode", () => { loaded.assistant++; return { default: () => <div>Assistant</div> }; });
import WindowEntry from "./WindowEntry";

function render(search: string) {
  return new Promise<string>((resolve, reject) => {
    let html = "";
    const destination = new Writable({ write(chunk, _encoding, done) { html += chunk; done(); } });
    destination.on("finish", () => resolve(html));
    destination.on("error", reject);
    const stream = renderToPipeableStream(<Suspense><WindowEntry search={search} /></Suspense>, {
      onAllReady() { stream.pipe(destination); }, onError: reject
    });
  });
}

describe("window entry loading", () => {
  it("loads only the selected window and preserves document precedence and decoding", async () => {
    expect(loaded).toEqual({ app: 0, document: 0, assistant: 0 });
    expect(await render("?document-chat=D%3A%5Cnotes%5C%E4%BC%9A%E8%AE%AE.md&assistant")).toContain("D:\\notes\\会议.md");
    expect(loaded).toEqual({ app: 0, document: 1, assistant: 0 });
    expect(await render("?document-chat=")).toContain("Document:");
    expect(await render("?assistant")).toContain("Assistant");
    expect(loaded).toEqual({ app: 0, document: 1, assistant: 1 });
    expect(await render("?unrelated=1")).toContain("Main window");
    expect(loaded.app).toBe(1);
  });
});
