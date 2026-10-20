"""Real Explorer selection / Tauri placement acceptance; dedicated QA app only."""
import base64
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import time
from urllib.request import urlopen
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / ".build" / "document-placement-qa"
EXE = ROOT / "src-tauri" / "target" / "debug" / "nano-desk.exe"
user32 = ctypes.windll.user32
user32.GetForegroundWindow.restype = wintypes.HWND
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]

def wait(fn, seconds=35):
    deadline = time.time() + seconds
    while time.time() < deadline:
        value = fn()
        if value: return value
        time.sleep(.15)
    raise AssertionError("Timed out")

def explorer_windows():
    found = []
    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        title = ctypes.create_unicode_buffer(512)
        cls = ctypes.create_unicode_buffer(128)
        user32.GetWindowTextW(hwnd, title, 512)
        user32.GetClassNameW(hwnd, cls, 128)
        if cls.value == "CabinetWClass" and OUT.name in title.value: found.append(hwnd)
        return True
    user32.EnumWindows(visit, 0)
    return found

def selection(hwnd):
    script = """Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$r=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]HWND_VALUE)
$c=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.SelectionItemPattern]::IsSelectedProperty,$true)
$items=$r.FindAll([System.Windows.Automation.TreeScope]::Descendants,$c)
foreach($i in $items) { if($i.Current.Name -like 'anchor-test*') { $b=$i.Current.BoundingRectangle; @{x=$b.X;y=$b.Y;width=$b.Width;height=$b.Height}|ConvertTo-Json -Compress; break } }
""".replace("HWND_VALUE", str(hwnd))
    encoded = base64.b64encode(script.encode("utf-16le")).decode()
    result = subprocess.run(["powershell.exe", "-NoProfile", "-EncodedCommand", encoded], capture_output=True, text=True, creationflags=subprocess.CREATE_NO_WINDOW, check=True)
    return json.loads(result.stdout) if result.stdout.strip() else None

def invoke(page, command):
    return page.evaluate("async command => (await import('/node_modules/@tauri-apps/api/core.js')).invoke(command)", command)

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    file = OUT / "anchor-test.txt"
    file.write_text("Document placement acceptance.", encoding="utf-8")
    existing = set(explorer_windows())
    subprocess.Popen(["explorer.exe", "/select," + str(file)])
    hwnd = wait(lambda: next(iter(set(explorer_windows()) - existing), None))
    process = None
    try:
        user32.SetForegroundWindow(hwnd)
        wait(lambda: user32.GetForegroundWindow() == hwnd)
        item = wait(lambda: selection(hwnd))
        user32.SetForegroundWindow(hwnd)
        wait(lambda: user32.GetForegroundWindow() == hwnd)
        env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9224")
        process = subprocess.Popen([str(EXE), "--document-chat", str(file)], env=env, creationflags=subprocess.CREATE_NO_WINDOW)
        def endpoint():
            try: return json.load(urlopen("http://127.0.0.1:9224/json/version", timeout=1))
            except OSError: return None
        wait(endpoint)
        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp("http://127.0.0.1:9224")
            def page():
                for context in browser.contexts:
                    if context.pages: context.pages[0].wait_for_timeout(100)
                return next((tab for context in browser.contexts for tab in context.pages if "document-chat=" in tab.url), None)
            doc = wait(page)
            doc.wait_for_load_state("networkidle")
            pos = invoke(doc, "plugin:window|outer_position")
            size = invoke(doc, "plugin:window|outer_size")
            item = selection(hwnd)
            adjacent = min(abs(pos["x"] - item["x"] - item["width"]), abs(item["x"] - pos["x"] - size["width"]), abs(pos["y"] - item["y"] - item["height"]), abs(item["y"] - pos["y"] - size["height"]))
            assert adjacent <= 40, ("Window is not beside the selected file", item, pos, size)
            overlap = pos["x"] < item["x"]+item["width"] and item["x"] < pos["x"]+size["width"] and pos["y"] < item["y"]+item["height"] and item["y"] < pos["y"]+size["height"]
            assert not overlap, (item, pos, size)
            doc.screenshot(path=str(OUT / "document-beside-selection.png"))
            print("PASS Explorer selected icon/name do not overlap the document window", item, pos, size, flush=True)
            user32.SetForegroundWindow(hwnd)
            wait(lambda: user32.GetForegroundWindow() == hwnd)
            subprocess.Popen([str(EXE), "--document-chat", str(file)], creationflags=subprocess.CREATE_NO_WINDOW).wait(timeout=15)
            def second_page():
                doc.wait_for_timeout(100)
                return next((tab for context in browser.contexts for tab in context.pages if "document-chat=" in tab.url and tab != doc), None)
            second = wait(second_page)
            second.wait_for_load_state("networkidle")
            pos = invoke(second, "plugin:window|outer_position")
            size = invoke(second, "plugin:window|outer_size")
            item = selection(hwnd)
            adjacent = min(abs(pos["x"] - item["x"] - item["width"]), abs(item["x"] - pos["x"] - size["width"]), abs(pos["y"] - item["y"] - item["height"]), abs(item["y"] - pos["y"] - size["height"]))
            assert adjacent <= 40, ("Window is not beside the selected file", item, pos, size)
            overlap = pos["x"] < item["x"]+item["width"] and item["x"] < pos["x"]+size["width"] and pos["y"] < item["y"]+item["height"] and item["y"] < pos["y"]+size["height"]
            assert not overlap, (item, pos, size)
            print("PASS warm single-instance launch also avoids the selected file", flush=True)
    finally:
        if process:
            process.terminate()
            process.wait(timeout=10)
        if hwnd not in existing: user32.PostMessageW(hwnd, 0x0010, 0, 0)

if __name__ == "__main__": main()
