"""Smoke-test the real App shell with simulated desktop IPC."""
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

MOCK = r"""
let callbackId=0;
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
window.__TAURI_INTERNALS__={
 metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},
 transformCallback:()=>++callbackId, unregisterCallback:()=>{},
 invoke:async(command,args={})=>{
  if(command==='list_conversations') return [{id:'a',title:'摘要设计会话',archived:false,project_path:null,model_config_id:null,created_at:'2026-10-15T00:00:00Z',updated_at:'2026-10-15T00:00:00Z'}];
  if(command==='list_messages') return [
   {id:'u',conversation_id:'a',role:'user',content:'先设计任务摘要，不实施。',created_at:'2026-10-15T00:00:00Z'},
   {id:'s',conversation_id:'a',role:'system',content:'内部压缩记录不应该出现在界面',metadata:{context_summary:{version:1,covered_through_message_id:'u',covered_message_count:1}},created_at:'2026-10-15T00:00:01Z'},
   {id:'r',conversation_id:'a',role:'assistant',content:'我们先确认摘要设计方案。',created_at:'2026-10-15T00:00:02Z'}];
  if(command==='get_task_summary_items') return ['设计任务摘要与上下文压缩', '先设计，暂不实施', '一个聊天维护一条主任务'];
  if(command==='list_local_skills') return ['D:/skills',[]];
  if(command==='restore_mcp_servers') return [];
  if(command.startsWith('list_') || command.startsWith('search_')) return [];
  if(command==='check_env') return {node:true,python:true,paddleocr:true};
  if(command==='get_tavily_api_key') return '';
  if(command.startsWith('plugin:event|')) return ++callbackId;
  if(command==='plugin:window|is_focused') return true;
  return null;
 }
};
"""
with sync_playwright() as p:
    browser=p.chromium.launch(channel="msedge",headless=True)
    page=browser.new_page(viewport={"width":1440,"height":900})
    errors=[]
    page.on("pageerror",lambda error:errors.append(error.stack))
    page.add_init_script(MOCK)
    page.goto("http://127.0.0.1:1420/")
    page.wait_for_load_state("networkidle")
    assert not errors, errors
    panel=page.get_by_role("complementary",name="任务摘要")
    expect(panel).to_be_visible()
    page.get_by_text("摘要设计会话",exact=True).first.click()
    expect(panel.get_by_role("listitem")).to_have_count(3)
    expect(page.get_by_text("内部压缩记录不应该出现在界面")).to_have_count(0)
    expect(page.get_by_text("先设计任务摘要，不实施。",exact=True)).to_be_visible()
    expect(page.get_by_text("我们先确认摘要设计方案。",exact=True)).to_be_visible()
    for width in [1440,960]:
        page.set_viewport_size({"width":width,"height":900})
        boxes=[page.locator(selector).bounding_box() for selector in [".sidebar",".chat-pane",".task-summary-panel"]]
        assert boxes[0]["x"]+boxes[0]["width"]<=boxes[1]["x"]+1, boxes
        assert boxes[1]["x"]+boxes[1]["width"]<=boxes[2]["x"]+1, boxes
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
        output=Path(".build/task-summary"); output.mkdir(parents=True,exist_ok=True)
        page.screenshot(path=str(output/f"app-{width}.png"))
    page.get_by_role("button",name="收起任务摘要").click()
    expect(panel.get_by_role("list")).to_be_hidden()
    assert not errors, errors
    browser.close()
    print("Real App: three columns at 1440/960, summary items, transcript preservation, compression hidden and collapse passed.")
