"""Run against Vite on port 1420; exercises real React UI with mocked Tauri IPC."""
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

MOCK = """
window.makeRun = (goal, status='awaiting_plan') => ({id:goal, status, plan_json:JSON.stringify({goal,steps:[{id:'design',title:'设计步骤',status:'pending'},{id:'verify',title:'验证结果',status:'pending'}]})});
window.taskItems = {a: [], b: [window.makeRun('另一个会话的任务')]};
window.decisions = [];
window.delayA = false; window.fail = false;
window.__TAURI_INTERNALS__ = {invoke: async (command, args) => {
  if(command === 'respond_background_agent') {
    window.decisions.push(args.decision);
    const run = window.taskItems.a[0];
    run.status = args.decision.action === 'approve_plan' ? 'running' : 'cancelled';
    return;
  }
  if(command !== 'list_agent_runs') throw Error(command);
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
    panel = page.get_by_role("complementary", name="任务规划")
    expect(panel.get_by_text("暂无任务规划")).to_be_visible()
    separator = panel.get_by_role("separator", name="调整任务规划宽度")
    before = panel.bounding_box()["width"]
    handle = separator.bounding_box()
    page.mouse.move(handle["x"] + handle["width"] / 2, handle["y"] + 100)
    page.mouse.down()
    page.mouse.move(handle["x"] - 120, handle["y"] + 100, steps=10)
    page.mouse.up()
    expanded_width = panel.bounding_box()["width"]
    assert expanded_width > before + 100
    page.reload(); page.wait_for_load_state("networkidle")
    assert abs(panel.bounding_box()["width"] - expanded_width) < 2
    separator.focus()
    separator.press("ArrowRight")
    assert panel.bounding_box()["width"] < expanded_width
    separator.press("End")
    assert page.locator("main").bounding_box()["width"] >= 360
    separator.dblclick()
    assert abs(panel.bounding_box()["width"] - before) < 2
    page.evaluate("window.taskItems.a = [window.makeRun('实现任务规划')] ")
    page.get_by_role("button", name="完成一轮").click()
    expect(panel.get_by_role("listitem")).to_have_count(2)
    expect(panel.get_by_text("设计步骤", exact=True)).to_be_visible()
    boxes = [page.locator(selector).bounding_box() for selector in ["nav", "main", ".task-summary-panel"]]
    assert boxes[0]["x"] + boxes[0]["width"] <= boxes[1]["x"] + 1
    assert boxes[1]["x"] + boxes[1]["width"] <= boxes[2]["x"] + 1
    expect(panel.get_by_text('等待用户审核，尚未执行', exact=False)).to_be_visible()
    panel.get_by_role('button', name='批准规划并执行').click()
    page.wait_for_function("window.decisions.length === 1")
    expect(panel.get_by_role('button', name='批准规划并执行')).to_have_count(0)
    page.evaluate("const r=window.taskItems.a[0]; const p=JSON.parse(r.plan_json); p.steps[0].status='completed'; p.steps[1].status='in_progress'; r.plan_json=JSON.stringify(p)")
    expect(panel.get_by_text('已完成 1/2', exact=False)).to_be_visible()
    expect(panel.get_by_text('进行中', exact=True)).to_be_visible()
    # A new task collapses the previous plan even before its own plan is ready.
    page.evaluate("window.taskItems.a[0].status='completed'; window.taskItems.a.unshift({id:'second',status:'running',plan_json:null})")
    expect(panel.get_by_text('正在生成规划', exact=True)).to_be_visible()
    previous = panel.locator('details').filter(has_text='实现任务规划')
    expect(previous).not_to_have_attribute('open', '')
    page.evaluate("window.taskItems.a[0] = window.makeRun('第二个任务')")
    latest = panel.locator('details').filter(has_text='第二个任务')
    expect(latest).to_have_attribute('open', '')
    expect(previous).not_to_have_attribute('open', '')
    previous.locator('summary').click()
    expect(previous).to_have_attribute('open', '')
    page.wait_for_timeout(1200)
    expect(previous).to_have_attribute('open', '')
    output = Path("output/playwright")
    output.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(output / "expanded.png"))
    page.get_by_role("button", name="收起任务规划").click()
    expect(panel.locator(".task-summary-content")).to_be_hidden()
    page.reload(); page.wait_for_load_state("networkidle")
    expect(panel.get_by_role("button", name="展开任务规划").first).to_be_visible()
    page.get_by_role("button", name="展开任务规划").first.click()
    assert abs(panel.bounding_box()["width"] - before) < 2
    page.evaluate("window.delayA = true")
    page.get_by_role("button", name="完成一轮").click()
    page.wait_for_function("!!window.resolveA")
    page.get_by_role("button", name="会话 B").click()
    expect(panel.get_by_text("另一个会话的任务")).to_be_visible()
    page.evaluate("window.resolveA([window.makeRun('过期会话结果')])")
    expect(panel.get_by_text("过期会话结果")).to_have_count(0)
    page.evaluate("window.fail = true")
    page.get_by_role("button", name="完成一轮").click()
    expect(panel.get_by_text("read failed", exact=False)).to_be_visible()
    page.evaluate("window.fail = false")
    panel.get_by_role("button", name="重新读取").click()
    expect(panel.get_by_text("另一个会话的任务")).to_be_visible()
    page.set_viewport_size({"width": 960, "height": 800})
    page.get_by_role("button", name="收起任务规划").click()
    expect(panel.get_by_role("button", name="展开任务规划").first).to_be_visible()
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    # The desktop shell has a 960px minimum; isolate the panel's overlay layout below it.
    page.add_style_tag(content="body { min-width: 0; }")
    page.set_viewport_size({"width": 640, "height": 800})
    page.get_by_role("button", name="展开任务规划").first.click()
    assert panel.bounding_box()["width"] <= page.locator(".chat-workspace").bounding_box()["width"] * 0.85 + 1
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    browser.close()
    print("Task summary UI: empty state, items, three columns, collapse persistence, conversation isolation, stale response, retry and narrow layout passed.")
