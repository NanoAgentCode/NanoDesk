"""Production-window loading smoke test with simulated Tauri IPC.

Run against `npm run preview -- --host 127.0.0.1 --port 1431`.
"""
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

MOCK = r"""
let next = 0;
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: () => {}};
window.__TAURI_INTERNALS__ = {
  metadata: {currentWindow: {label: 'main'}, currentWebview: {label: 'main'}},
  transformCallback: () => ++next, unregisterCallback: () => {},
  invoke: async (command, args = {}) => {
    if (command === 'list_local_skills') return ['D:/skills', []];
    if (command === 'check_env') return {node: true, python: true, paddleocr: true};
    if (command === 'restore_mcp_servers' || command.startsWith('list_') || command.startsWith('search_')) return [];
    if (command === 'plugin:window|is_focused') return true;
    if (command.startsWith('plugin:event|')) return ++next;
    if (command === 'read_document_chat_file') return {name: 'smoke.md', path: args.path, content: 'Document smoke fixture', extension: 'md'};
    if (command === 'get_tavily_api_key') return '';
    return null;
  }
};
"""

output = Path(__file__).resolve().parents[2] / '.build/window-entry-qa'
output.mkdir(parents=True, exist_ok=True)
results = []
with sync_playwright() as p:
    browser = p.chromium.launch(channel='msedge', headless=True)
    try:
        for name, query, selected, forbidden, selector in [
            ('assistant', '?assistant', 'AssistantMode-', ['App-', 'DocumentChat-', 'vendor-markdown-'], '.assistant-orb'),
            ('document', '?document-chat=D%3A%5Csmoke.md&assistant', 'DocumentChat-', ['App-', 'AssistantMode-'], '.document-chat'),
            ('main', '', 'App-', ['DocumentChat-', 'AssistantMode-'], '.sidebar'),
        ]:
            context = browser.new_context(viewport={'width': 1280, 'height': 820})
            context.add_init_script(MOCK)
            page = context.new_page()
            errors, scripts = [], []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('request', lambda request: scripts.append(request.url) if request.resource_type == 'script' else None)
            page.goto('http://127.0.0.1:1431/' + query)
            page.wait_for_load_state('networkidle')
            expect(page.locator(selector).first).to_be_visible()
            expect(page.locator('#nano-startup')).to_have_count(0)
            assert not errors, (name, errors)
            assert any(selected in url for url in scripts), (name, scripts)
            assert not any(item in url for item in forbidden for url in scripts), (name, scripts)
            page.screenshot(path=str(output / f'{name}.png'))
            results.append({'window': name, 'scripts': scripts, 'errors': errors})
            context.close()
    finally:
        browser.close()
(output / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding='utf-8')
print('PASS: production windows render without JS errors, load only their selected entry, and dismiss startup overlay.')
