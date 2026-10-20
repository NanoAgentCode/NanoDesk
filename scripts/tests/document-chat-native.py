"""Native Tauri/WebView2 acceptance. Build with TAURI_CONFIG identifier com.nanodesk.document-chat-qa.

Run fixture generator with bundled Python first, then run this script with a
Playwright-enabled Python and Vite listening on port 1420. Only the dedicated QA
app database is used. The document verb registry entries are backed up/restored.
"""
import ctypes
import json
import os
import re
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
embedding_requests = []
fail_embedding_at = None


class Model(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path.endswith("/embeddings"):
            embedding_requests.append(body)
            if fail_embedding_at == len(embedding_requests):
                self.send_response(503); self.end_headers(); self.wfile.write(b"QA embedding failure"); return
            self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
            inputs = body["input"]
            vectors = [{"index": i, "embedding": [1.0 if "TAIL-LARGE-8972" in text or "验证码" in text else 0.0, 0.1, 0.1]} for i, text in enumerate(inputs)]
            self.wfile.write(json.dumps({"data": vectors}).encode()); return
        requests.append(body)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if body.get("stream") else "application/json")
        self.end_headers()
        if not body.get("stream"):
            text = "\n".join(message["content"] for message in body["messages"])
            facts = sorted(set(re.findall(r"TODO-\d{4}|TAIL-LARGE-8972", text)))
            response = "逐块事实：" + "、".join(facts) if facts else "本组未发现待办。"
            self.wfile.write(json.dumps({"choices": [{"message": {"role": "assistant", "content": response}}]}).encode())
            return
        slow = "长回复" in body["messages"][-1]["content"]
        all_context = "\n".join(message["content"] for message in body["messages"])
        answer = "文档末尾是 TAIL-LARGE-8972。" if "TAIL-LARGE-8972" in all_context else "文档末尾是 TAIL-8972。"
        try:
            for part in (["正在生成…"] * 100 if slow else ["QA 回答：", answer]):
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
    global fail_embedding_at
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
            def first_document():
                for context in browser.contexts:
                    if context.pages:
                        context.pages[0].wait_for_timeout(100)
                        break
                return next(iter(document_pages()), None)
            doc = wait_until(first_document)
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
            for configured in invoke(doc, "list_model_configs"):
                if configured["id"] == "embedding-config": invoke(doc, "delete_model_config", {"id": configured["id"]})
            main_page.reload(); main_page.wait_for_load_state("networkidle")
            doc.reload()
            doc.wait_for_load_state("networkidle")
            expect(doc.get_by_text("想了解这份文档的什么？", exact=True)).to_be_visible()
            initial_size = invoke(doc, "plugin:window|inner_size")
            assert initial_size == {"width": 764, "height": 1100}, initial_size
            doc.screenshot(path=str(OUT / "document-chat-default-size.png"))
            doc.get_by_role("button", name="总结全文", exact=True).click()
            expect(doc.get_by_role("textbox", name="输入消息")).to_have_value("总结这份文档的核心内容")
            expect(doc.get_by_role("textbox", name="输入消息")).to_have_attribute("placeholder", "针对这份文档提问…")
            doc.set_viewport_size({"width": 340, "height": 370})
            expect(doc.get_by_role("textbox", name="输入消息")).to_be_visible()
            assert doc.evaluate("document.documentElement.scrollWidth <= innerWidth"), "Overflow at minimum window size"
            doc.screenshot(path=str(OUT / "document-chat-minimum.png"))
            doc.set_viewport_size({"width": 340, "height": 370})
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
            assert "TAIL-8972" not in history[0]["content"]
            files = invoke(doc, "list_rag_files", {"conversationId": session["id"]})
            assert len(files) == 1 and files[0]["status"] == "fulltext"
            assert any("文档开头" in message["content"] and "TAIL-8972" in message["content"] for message in requests[-1]["messages"])
            assert requests[-1]["messages"][-1]["content"] == history[0]["content"]
            field.fill("继续解释刚才的验证码")
            field.press("Enter")
            expect(doc.locator(".chat-message.assistant")).to_have_count(2, timeout=25000)
            wait_until(lambda: not invoke(doc, "list_background_agents"))
            assert len(invoke(doc, "list_conversations")) == baseline + 1
            assert any("TAIL-8972" in message["content"] for message in requests[-1]["messages"][:-1])
            assert len(invoke(doc, "list_rag_files", {"conversationId": session["id"]})) == 1
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
            doc.get_by_role("button", name="主窗口继续", exact=True).click()
            assert not invoke(doc, "plugin:window|is_visible"), "Document window must hide when main opens"
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
            assert not embedding_requests, "Short documents must not require embeddings"

            # Exercise the long-document path, including the old 120-chunk boundary.
            large_path = OUT / "large-rag.txt"
            large_path.write_text("\n".join(f"SECTION-{i:04d} TODO-{i:04d}: " + "Background context for document verification. " * 44 for i in range(125)) + "\n最后的验证码是 TAIL-LARGE-8972", encoding="utf-8")
            subprocess.Popen([str(EXE), "--document-chat", str(large_path)], creationflags=subprocess.CREATE_NO_WINDOW).wait(timeout=10)
            def large_document():
                main_page.wait_for_timeout(100)
                return next((page for page in document_pages() if "large-rag.txt" in page.url), None)
            large = wait_until(large_document)
            large.wait_for_load_state("networkidle")
            large.evaluate("async () => {window.indexEvents=[]; await (await import('/node_modules/@tauri-apps/api/event.js')).listen('document-index-progress', event => window.indexEvents.push(event.payload));}")
            question = "文档最后的验证码是什么？"
            large.get_by_role("textbox", name="输入消息").fill(question)
            large.get_by_role("button", name="发送", exact=True).click()
            expect(large.get_by_role("alert")).to_contain_text("配置嵌入模型", timeout=15000)
            expect(large.get_by_role("textbox", name="输入消息")).to_have_value(question)
            rag_session = invoke(large, "list_conversations")[0]
            assert not invoke(large, "list_rag_files", {"conversationId": rag_session["id"]})
            invoke(large, "save_model_config", {"draft": {"id": "embedding-config", "name": "QA embedding", "provider": "openai-compatible", "base_url": f"http://127.0.0.1:{server.server_port}/v1", "model": "qa-embedding", "api_key": "qa", "embedding_provider": "openai-compatible", "embedding_base_url": f"http://127.0.0.1:{server.server_port}/v1", "embedding_model": "qa-embedding", "embedding_api_key": "qa", "model_kind": "embedding"}})
            fail_embedding_at = 2
            large.get_by_role("button", name="重试文档准备").click()
            expect(large.get_by_role("alert")).to_contain_text("503", timeout=20000)
            assert not invoke(large, "list_rag_files", {"conversationId": rag_session["id"]}), "Failed batch left a partial index"
            expect(large.get_by_role("textbox", name="输入消息")).to_have_value(question)
            fail_embedding_at = None
            large.get_by_role("button", name="重试文档准备").click()
            expect(large.locator(".chat-message.assistant")).to_contain_text("TAIL-LARGE-8972", timeout=60000)
            wait_until(lambda: not invoke(large, "list_background_agents"))
            indexed = invoke(large, "list_rag_files", {"conversationId": rag_session["id"]})
            assert len(indexed) == 1 and indexed[0]["chunk_count"] > 120 and indexed[0]["status"] == "ready"
            assert large.evaluate("window.indexEvents.some(event => event.phase==='indexing' && event.completed>=16 && event.total>120)")
            expect(large.locator(".document-chat-source")).to_contain_text("索引已就绪")
            expect(large.locator(".document-chat-notice")).to_have_count(0)
            index_calls = len([request for request in embedding_requests if len(request["input"]) > 1])
            assert all(len(request["input"]) <= 16 for request in embedding_requests)
            prompt = next(request for request in reversed(requests) if request.get("stream"))
            assert any("TAIL-LARGE-8972" in message["content"] for message in prompt["messages"])
            assert sum(len(message["content"]) for message in prompt["messages"]) < len(large_path.read_text(encoding="utf-8")) // 2
            large.get_by_role("textbox", name="输入消息").fill("再次确认尾部验证码")
            large.get_by_role("button", name="发送", exact=True).click()
            expect(large.locator(".chat-message.assistant")).to_have_count(2, timeout=30000)
            wait_until(lambda: not invoke(large, "list_background_agents"))
            assert len([request for request in embedding_requests if len(request["input"]) > 1]) == index_calls
            assert invoke(large, "list_rag_files", {"conversationId": rag_session["id"]})[0]["id"] == indexed[0]["id"]
            before_summaries = len([request for request in requests if not request.get("stream")])
            large.get_by_role("textbox", name="输入消息").fill("列出全文所有待办事项")
            large.get_by_role("button", name="发送", exact=True).click()
            expect(large.locator(".chat-message.assistant")).to_have_count(3, timeout=90000)
            wait_until(lambda: not invoke(large, "list_background_agents"))
            summaries = [request for request in requests if not request.get("stream")][before_summaries:]
            assert len(summaries) > 1
            analyzed = "\n".join(message["content"] for request in summaries for message in request["messages"])
            for index in range(125): assert f"TODO-{index:04d}" in analyzed
            final_prompt = next(request for request in reversed(requests) if request.get("stream"))
            assert any("全部" in message["content"] and "分块" in message["content"] for message in final_prompt["messages"])
            large.screenshot(path=str(OUT / "document-rag-ready.png"))
            large.get_by_role("button", name="主窗口继续", exact=True).click()
            assert not invoke(large, "plugin:window|is_visible"), "Document window must hide when main opens"
            expect(main_page.locator(".chat-message.user").first).to_contain_text(question, timeout=10000)
            main_page.get_by_role("textbox", name="输入消息").fill("尾部验证码再确认")
            main_page.get_by_role("button", name="发送", exact=True).click()
            expect(main_page.locator(".chat-message.assistant")).to_have_count(4, timeout=30000)
            wait_until(lambda: not invoke(main_page, "list_background_agents"))
            assert len([request for request in embedding_requests if len(request["input"]) > 1]) == index_calls
            attachment = OUT / "extra-short.txt"
            attachment.write_text("附件联系人是小林 SHORT-ATTACHMENT", encoding="utf-8")
            main_page.locator('input[type="file"]').set_input_files(str(attachment))
            wait_until(lambda: len(invoke(main_page, "list_rag_files", {"conversationId": rag_session["id"]})) == 2)
            assert any(file["name"] == attachment.name and file["status"] == "fulltext" for file in invoke(main_page, "list_rag_files", {"conversationId": rag_session["id"]}))
            main_page.wait_for_timeout(300)
            main_page.locator('input[type="file"]').set_input_files(str(large_path))
            expect(main_page.locator('.rag-file-chip')).to_have_count(2)
            main_page.wait_for_timeout(500)
            assert len([request for request in embedding_requests if len(request["input"]) > 1]) == index_calls
            assert next(file for file in invoke(main_page, "list_rag_files", {"conversationId": rag_session["id"]}) if file["name"] == large_path.name)["id"] == indexed[0]["id"]
            print("PASS long-document embedding batches, missing-config and mid-batch retry, tail retrieval beyond 120 chunks, index deduplication, full-coverage analysis and main-window reuse", flush=True)
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
