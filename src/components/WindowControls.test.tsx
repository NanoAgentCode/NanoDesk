import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
const actions = vi.hoisted(() => ({ minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => actions }));
import WindowControls from "./WindowControls";

describe("WindowControls", () => {
  it("provides the three native window actions", () => {
    const controls = WindowControls();
    const html = renderToStaticMarkup(controls);
    for (const label of ["最小化", "最大化或还原", "关闭"]) expect(html).toContain(label);
    controls.props.children[1].props.onClick();
    controls.props.children[2].props.onClick();
    controls.props.children[3].props.onClick();
    expect(actions.minimize).toHaveBeenCalledOnce();
    expect(actions.toggleMaximize).toHaveBeenCalledOnce();
    expect(actions.close).toHaveBeenCalledOnce();
  });
});
