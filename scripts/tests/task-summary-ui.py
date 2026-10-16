"""Run against Vite on port 1420; exercises real React UI with mocked Tauri IPC."""
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

MOCK = """
window.taskItems = {a: [], b: ['另一个会话的任务']};
window.delayA = false; window.fail = false;
window.__TAURI_INTERNALS__ = {invoke: async (command, args) => {
  if(command !== 'get_task_summary_items') throw Error(command);
  if(window.fail) throw Error('read failed');
  if(window.delayA && args.conversationId === 'a') return new Promise(resolve => window.resolveA = resolve);
  return window.taskItems[args.conversationId];
}};
"""
with sync_playwright() as p:
    browser = p.chromium.launch(channel="msedge", headless=True)
    page = browser.new_page(viewport={"width": 1366, "height": 900})
    page.add_init_script(MOCK)
    page.goto("http://127.0.0.1:1420/scripts/tests/task-summary.html")
    page.wait_for_load_state("networkidle")
    panel = page.get_by_role("complementary", name="任务摘要")
    expect(panel.get_by_text("还没有明确的任务")).to_be_visible()
    page.evaluate("window.taskItems.a = ['设计任务摘要与上下文压缩', '只设计，暂不实施', '每个聊天保留一条主任务']")
    page.get_by_role("button", name="完成一轮").click()
    expect(panel.get_by_role("listitem")).to_have_count(3)
    expect(panel.get_by_text("只设计，暂不实施")).to_be_visible()
    boxes = [page.locator(selector).bounding_box() for selector in ["nav", "main", ".task-summary-panel"]]
    assert boxes[0]["x"] + boxes[0]["width"] <= boxes[1]["x"] + 1
    assert boxes[1]["x"] + boxes[1]["width"] <= boxes[2]["x"] + 1
    output = Path(".build/task-summary")
    output.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(output / "expanded.png"))
    page.get_by_role("button", name="收起任务摘要").click()
    expect(panel.get_by_role("list")).to_be_hidden()
    page.reload(); page.wait_for_load_state("networkidle")
    expect(panel.get_by_role("button", name="展开任务摘要").first).to_be_visible()
    page.get_by_role("button", name="展开任务摘要").first.click()
    page.evaluate("window.delayA = true")
    page.get_by_role("button", name="完成一轮").click()
    page.wait_for_function("!!window.resolveA")
    page.get_by_role("button", name="会话 B").click()
    expect(panel.get_by_text("另一个会话的任务")).to_be_visible()
    page.evaluate("window.resolveA(['过期会话结果'])")
    expect(panel.get_by_text("过期会话结果")).to_have_count(0)
    page.evaluate("window.fail = true")
    page.get_by_role("button", name="完成一轮").click()
    expect(panel.get_by_text("暂时无法读取任务摘要。")).to_be_visible()
    page.evaluate("window.fail = false")
    panel.get_by_role("button", name="重新读取").click()
    expect(panel.get_by_text("另一个会话的任务")).to_be_visible()
    page.set_viewport_size({"width": 960, "height": 800})
    page.get_by_role("button", name="收起任务摘要").click()
    expect(panel.get_by_role("button", name="展开任务摘要").first).to_be_visible()
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    browser.close()
    print("Task summary UI: empty state, items, three columns, collapse persistence, conversation isolation, stale response, retry and narrow layout passed.")
