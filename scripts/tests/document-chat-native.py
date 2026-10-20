"""Native Tauri/WebView2 acceptance. Build with TAURI_CONFIG identifier com.nanodesk.document-chat-qa.

Run fixture generator with bundled Python first, then run this script with a
Playwright-enabled Python and Vite listening on port 1420. Only the dedicated QA
app database is used. The document verb registry entries are backed up/restored.
"""
import ctypes
import json
import os
from pathlib import Path
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import urlopen
import winreg
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / ".build" / "document-chat-qa"
EXE = ROOT / "src-tauri" / "target" / "debug" / "nano-desk.exe"
EXTENSIONS = ["txt", "md", "markdown", "docx", "pdf", "rtf", "csv", "xlsx", "pptx"]
requests = []


class Model(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests.append(body)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if body.get("stream") else "application/json")
        self.end_headers()
        if not body.get("stream"):
            self.wfile.write(json.dumps({"choices": [{"message": {"role": "assistant", "content": "{}"}}]}).encode())
            return
        slow = "长回复" in body["messages"][-1]["content"]
        try:
            for part in (["正在生成…"] * 100 if slow else ["QA 回答：", "文档末尾是 TAIL-8972。"]):
                data = {"choices": [{"delta": {"content": part}, "finish_reason": None}]}
                self.wfile.write(("data: " + json.dumps(data, ensure_ascii=False) + "\n\n").encode())
                self.wfile.flush()
                time.sleep(.15)
            self.wfile.write(b'data: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass


def registry_path(ext):
    return fr"Software\Classes\SystemFileAssociations\.{ext}\shell\NanoDesk.DocumentChat"


def registry_snapshot(path):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            values, children = [], {}
            for index in range(winreg.QueryInfoKey(key)[1]):
                values.append(winreg.EnumValue(key, index))
            for index in range(winreg.QueryInfoKey(key)[0]):
                name = winreg.EnumKey(key, index)
                children[name] = registry_snapshot(path + "\\" + name)
            return values, children
    except FileNotFoundError:
        return None


def restore_registry(path, snapshot):
    if snapshot is None:
        return
    values, children = snapshot
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, path) as key:
        for name, value, kind in values:
            winreg.SetValueEx(key, name, 0, kind, value)
    for name, child in children.items():
        restore_registry(path + "\\" + name, child)


def invoke(page, command, args=None):
    return page.evaluate("async ([command,args]) => (await import('/node_modules/@tauri-apps/api/core.js')).invoke(command,args)", [command, args or {}])


def wait_until(check, timeout=25):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.1)
    raise AssertionError("Timed out waiting for native acceptance condition")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223")
    log = (OUT / "native-process.log").open("w", encoding="utf-8")
    process = subprocess.Popen([str(EXE), "--document-chat", str(OUT / "中文 资料.txt")], env=env, stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW)
    backup = {ext: registry_snapshot(registry_path(ext)) for ext in EXTENSIONS}
    registry_touched = False
    doc = None
    try:
        def endpoint():
            assert process.poll() is None, "QA app exited before CDP was ready"
            try:
                return json.load(urlopen("http://127.0.0.1:9223/json/version", timeout=1))
            except OSError:
                return None
        wait_until(endpoint, 35)
        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp("http://127.0.0.1:9223")
            def document_pages():
                return [page for context in browser.contexts for page in context.pages if "document-chat=" in page.url]
            doc = wait_until(lambda: next(iter(document_pages()), None))
            doc.wait_for_load_state("networkidle")
            identifier = doc.evaluate("async () => (await import('/node_modules/@tauri-apps/api/app.js')).getIdentifier()")
            assert identifier == "com.nanodesk.document-chat-qa", f"Refusing to mutate non-QA app: {identifier}"
            main_page = next(page for context in browser.contexts for page in context.pages if page != doc)
            assert not main_page.evaluate("async () => (await import('/node_modules/@tauri-apps/api/window.js')).getCurrentWindow().isVisible()")
            for name, expected in [("中文 资料.txt", "TAIL-8972"), ("中文 资料.md", "TAIL-8972"), ("中文 资料.csv", "TAIL-8972"), ("utf16.txt", "中文 UTF16"), ("gbk.txt", "中文 GBK"), ("example.rtf", "中文"), ("example.docx", "TABLE-8972"), ("example.xlsx", "TAIL-8972"), ("example.pdf", "TAIL-8972"), ("example.pptx", "SLIDE-11")]:
                extracted = invoke(doc, "read_document_chat_file", {"path": str(OUT / name)})
                assert expected in extracted["content"], (name, extracted["content"])
                if name == "example.docx":
                    assert "A & B < C" in extracted["content"] and "<w:" not in extracted["content"]
                if name == "example.pptx":
                    assert extracted["content"].index("SLIDE-02") < extracted["content"].index("SLIDE-10")
            for name in ["empty.txt", "scan.pdf", "missing.txt"]:
                try:
                    invoke(doc, "read_document_chat_file", {"path": str(OUT / name)})
                    raise AssertionError(f"Accepted invalid fixture {name}")
                except Exception as error:
                    assert "Accepted invalid fixture" not in str(error)
            print("PASS full extraction across all eight formats, Unicode, entities and failures", flush=True)
            model = invoke(doc, "save_model_config", {"draft": {"id": "document-chat-qa", "name": "本地验收模型", "provider": "openai-compatible", "base_url": f"http://127.0.0.1:{server.server_port}/v1", "model": "qa", "api_key": "qa-local-only", "context_window": 32768, "max_tokens": 2048, "model_kind": "chat"}})
            prefix = json.loads((ROOT / "brand.config.json").read_text(encoding="utf-8"))["storagePrefix"]
            doc.evaluate("([prefix,id]) => {localStorage.setItem(prefix+'-access-mode','auto');localStorage.setItem(prefix+'-fixed-models',JSON.stringify([id]));localStorage.setItem(prefix+'-smart-routing-enabled','false');}", [prefix, model["id"]])
            doc.reload()
            doc.wait_for_load_state("networkidle")
            expect(doc.get_by_role("heading", name="从这份文档开始聊")).to_be_visible()
            doc.set_viewport_size({"width": 480, "height": 520})
            expect(doc.get_by_role("textbox", name="输入消息")).to_be_visible()
            assert doc.evaluate("document.documentElement.scrollWidth <= innerWidth"), "Overflow at minimum window size"
            doc.screenshot(path=str(OUT / "document-chat-minimum.png"))
            doc.set_viewport_size({"width": 680, "height": 740})
            doc.screenshot(path=str(OUT / "document-chat-ready.png"))
            assert doc.evaluate("document.documentElement.scrollWidth <= innerWidth"), doc.evaluate("({width:innerWidth,body:document.body.className,overflow:[...document.querySelectorAll('*')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).slice(0,12).map(e=>({tag:e.tagName,cls:e.className,width:e.getBoundingClientRect().width}))})")
            baseline = len(invoke(doc, "list_conversations"))
            doc.screenshot(path=str(OUT / "document-chat-ready.png"))
            field = doc.get_by_role("textbox", name="输入消息")
            field.fill("文档最后的验证码是什么？")
            doc.get_by_role("button", name="发送", exact=True).click()
            expect(doc.locator(".chat-message.assistant").last).to_contain_text("TAIL-8972", timeout=25000)
            wait_until(lambda: not invoke(doc, "list_background_agents"))
            sessions = invoke(doc, "list_conversations")
            assert len(sessions) == baseline + 1
            session = sessions[0]
            assert session["title"] == "中文 资料.txt" and not session["project_path"]
            history = invoke(doc, "list_messages", {"conversationId": session["id"]})
            assert "文档开头" in history[0]["content"] and "TAIL-8972" in history[0]["content"]
            assert requests[-1]["messages"][-1]["content"] == history[0]["content"]
            field.fill("继续解释刚才的验证码")
            field.press("Enter")
            expect(doc.locator(".chat-message.assistant")).to_have_count(2, timeout=25000)
            wait_until(lambda: not invoke(doc, "list_background_agents"))
            assert len(invoke(doc, "list_conversations")) == baseline + 1
            assert any("TAIL-8972" in message["content"] for message in requests[-1]["messages"][:-1])
            assert sum("[NanoDesk 文档 " in message["content"] for message in requests[-1]["messages"]) == 1
            doc.screenshot(path=str(OUT / "document-chat-conversation.png"))
            print("PASS first-send session creation, full prompt, streaming and consecutive history", flush=True)
            field.fill("请生成长回复")
            field.press("Enter")
            expect(doc.locator(".chat-message.assistant").last).to_contain_text("正在生成", timeout=25000)
            stop = doc.get_by_role("button", name="打断", exact=True)
            expect(stop).to_be_visible()
            stop.click()
            wait_until(lambda: not invoke(doc, "list_background_agents"))
            assert invoke(doc, "list_messages", {"conversationId": session["id"]})[-1]["content"]
            doc.get_by_role("button", name="在主窗口继续", exact=True).click()
            expect(main_page.locator(".chat-message.user").first).to_contain_text("文档最后的验证码", timeout=15000)
            invoke(doc, "set_document_context_menu", {"enabled": True})
            registry_touched = True
            assert invoke(doc, "get_document_context_menu")
            for ext in EXTENSIONS:
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry_path(ext) + "\\command") as key:
                    assert winreg.QueryValueEx(key, "")[0] == f'"{EXE}" --document-chat "%1"'
            before = len(document_pages())
            # Execute the registered command. ShellExecute's extension-verb lookup
            # also consults default ProgIDs; Explorer's SystemFileAssociations
            # menu already has the resolved command used here.
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry_path("txt") + "\\command") as key:
                command = winreg.QueryValueEx(key, "")[0]
            parameters = command[len(f'"{EXE}" '):].replace("%1", str(OUT / "中文 资料.txt"))
            code = ctypes.windll.shell32.ShellExecuteW(None, "open", str(EXE), parameters, None, 1)
            assert code > 32, f"Shell verb failed: {code}"
            def new_document():
                doc.wait_for_timeout(100)  # Pump CDP target events while waiting for the new WebView.
                return next((page for page in document_pages() if page != doc), None)
            second = wait_until(new_document)
            second.wait_for_load_state("networkidle")
            assert len(document_pages()) == before + 1
            assert len(invoke(second, "list_conversations")) == baseline + 1
            second.get_by_role("textbox", name="输入消息").fill("独立会话测试")
            second.get_by_role("button", name="发送", exact=True).click()
            expect(second.locator(".chat-message.assistant")).to_contain_text("TAIL-8972", timeout=25000)
            wait_until(lambda: not invoke(second, "list_background_agents"))
            assert len(invoke(second, "list_conversations")) == baseline + 2
            second_session = invoke(second, "list_conversations")[0]
            second.get_by_role("textbox", name="输入消息").fill("关闭窗口时仍生成长回复")
            second.get_by_role("button", name="发送", exact=True).click()
            expect(second.locator(".chat-message.assistant").last).to_contain_text("正在生成", timeout=25000)
            second.evaluate("() => {void import('/node_modules/@tauri-apps/api/window.js').then(api=>api.getCurrentWindow().close());}")
            wait_until(lambda: not invoke(main_page, "list_background_agents"), 25)
            saved = invoke(main_page, "list_messages", {"conversationId": second_session["id"]})
            assert len(saved) == 4 and saved[-1]["content"].count("正在生成") == 100
            invoke(doc, "set_document_context_menu", {"enabled": False})
            assert not invoke(doc, "get_document_context_menu")
            for ext in EXTENSIONS:
                restore_registry(registry_path(ext), backup[ext])
            registry_touched = False
            doc.evaluate("() => {void import('/node_modules/@tauri-apps/api/window.js').then(api=>api.getCurrentWindow().close());}")
            expect(main_page.locator(".chat-message.user").first).to_contain_text("文档最后的验证码")
            assert len(invoke(main_page, "list_messages", {"conversationId": session["id"]})) >= 6
            print("PASS minimum-size layout, stop, main-window continuation, registry enable/disable, ShellExecute registered command, independent second session, background completion after window closes", flush=True)
            invoke(main_page, "quit_app")
    finally:
        if registry_touched:
            # Remove QA values even if the WebView has closed, then restore the exact snapshot.
            for ext in EXTENSIONS:
                path = registry_path(ext)
                try:
                    winreg.DeleteKey(winreg.HKEY_CURRENT_USER, path + "\\command")
                    winreg.DeleteKey(winreg.HKEY_CURRENT_USER, path)
                except FileNotFoundError:
                    pass
                restore_registry(path, backup[ext])
        if process.poll() is None:
            process.terminate()
        server.shutdown()
        log.close()


if __name__ == "__main__":
    main()
