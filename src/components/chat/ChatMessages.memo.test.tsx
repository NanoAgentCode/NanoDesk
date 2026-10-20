import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import type { ChatPaneProps } from "./types";
const hooks = vi.hoisted(() => ({ cursor: 0, values: [] as unknown[] }));
vi.mock("react", async importOriginal => ({
  ...await importOriginal<typeof import("react")>(),
  useRef: (value: unknown) => {
    const index = hooks.cursor++;
    return hooks.values[index] ?? (hooks.values[index] = { current: value });
  },
  useMemo: (compute: () => unknown, deps: unknown[]) => {
    const index = hooks.cursor++;
    const previous = hooks.values[index] as { deps: unknown[]; value: unknown } | undefined;
    if (previous && deps.every((dep, i) => Object.is(dep, previous.deps[i]))) return previous.value;
    const value = compute(); hooks.values[index] = { deps, value }; return value;
  }
}));
import ChatMessages from "./ChatMessages";
function render(props: ChatPaneProps) {
  hooks.cursor = 0;
  return ChatMessages(props).props.children as ReactElement<Record<string, unknown>>[];
}
describe("memoized message row props", () => {
  beforeEach(() => { hooks.cursor = 0; hooks.values = []; });
  it("keeps historical row props stable during streaming and dispatches the latest callbacks", async () => {
    const firstRetry = vi.fn(), latestRetry = vi.fn();
    const history = { id: "history", role: "assistant", content: "旧回答" };
    const props = { messages: [history, { id: "stream", role: "assistant", content: "新" }],
      messageReasoning: {}, messageToolCalls: {}, projectFiles: [], busy: true, activeStreamRequestId: "stream",
      handleRetryTool: firstRetry
    } as unknown as ChatPaneProps;
    const first = render(props);
    const second = render({ ...props, messages: [props.messages[0], { ...props.messages[1], content: "新回答" }], handleRetryTool: latestRetry });
    expect((first[0].type as { $$typeof?: symbol }).$$typeof).toBe(Symbol.for("react.memo"));
    const changedProps = (index: number) => Object.keys(first[index].props).filter(key => !Object.is(first[index].props[key], second[index].props[key]));
    expect(changedProps(0)).toEqual([]);
    expect(changedProps(1)).toEqual(["message"]);
    await (first[0].props.actions as { handleRetryTool: (id: string) => Promise<void> }).handleRetryTool("tool");
    expect(latestRetry).toHaveBeenCalledWith("tool");
    expect(firstRetry).not.toHaveBeenCalled();
  });
});
