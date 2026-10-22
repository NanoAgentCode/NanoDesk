"""Run against production preview on port 1432; validates task/files navigation."""
import ast
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

root = Path(__file__).resolve().parents[2]
fixture = ast.parse((root / 'scripts/tests/window-entry.py').read_text(encoding='utf-8'))
mock = next(ast.literal_eval(node.value) for node in fixture.body
            if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'MOCK' for target in node.targets))
output = root / '.build/task-panel-navigation'
output.mkdir(parents=True, exist_ok=True)

def assert_vertical_icons(panel):
    header = panel.locator('.task-summary-header')
    expect(header.get_by_role('button')).to_have_count(0)
    plan = header.get_by_role('tab', name='任务规划', exact=True)
    files = header.get_by_role('tab', name='交付文件', exact=True)
    boxes = [control.bounding_box() for control in [plan, files]]
    assert len(header.locator('svg').all()) == 2
    assert all(abs(box['x'] - boxes[0]['x']) < 1 for box in boxes), boxes
    assert boxes[0]['y'] < boxes[1]['y'], boxes
    assert plan.inner_text() == files.inner_text() == ''
    expect(header.get_by_role('tablist')).to_have_attribute('aria-orientation', 'vertical')

with sync_playwright() as p:
    browser = p.chromium.launch(channel='msedge', headless=True)
    try:
        for name, query in [('main', ''), ('document', '?document-chat=D%3A%5Csmoke.md')]:
            context = browser.new_context(viewport={'width': 1280, 'height': 820})
            context.add_init_script(mock)
            page = context.new_page()
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.goto('http://127.0.0.1:1432/' + query)
            page.wait_for_load_state('networkidle')
            panel = page.locator('.task-summary-panel')
            if name == 'document':
                expect(page.locator('.document-chat')).to_be_visible()
                expect(panel).to_have_count(0)
                assert not errors, errors
                context.close()
                continue
            tabs = panel.get_by_role('tablist')
            plan = tabs.get_by_role('tab', name='任务规划', exact=True)
            files = tabs.get_by_role('tab', name='交付文件', exact=True)
            expect(plan).to_be_visible()
            expect(files).to_be_visible()
            expect(tabs.get_by_role('tab')).to_have_count(2)
            assert plan.locator('svg').count() == files.locator('svg').count() == 1
            assert_vertical_icons(panel)
            expect(panel.get_by_role('heading', name='任务规划', exact=True)).to_be_visible()
            for icon, label in [(plan, '任务规划'), (files, '交付文件')]:
                if label == '交付文件':
                    icon.click()
                icon.click()
                expect(panel).to_have_class('task-summary-panel is-collapsed')
                assert_vertical_icons(panel)
                assert page.evaluate("localStorage.getItem('nanodesk-task-summary-collapsed')") == 'true'
                icon.click()
                expect(panel.get_by_role('heading', name=label, exact=True)).to_be_visible()
                assert page.evaluate("localStorage.getItem('nanodesk-task-summary-collapsed')") == 'false'
            plan.click()
            expect(panel.get_by_role('heading', name='任务规划', exact=True)).to_be_visible()
            files.click()
            expect(panel.get_by_role('heading', name='交付文件', exact=True)).to_be_visible()
            expect(panel.get_by_role('tabpanel', name='交付文件')).to_be_visible()
            expect(panel.get_by_text('用户上传', exact=True)).to_be_visible()
            expect(panel.get_by_text('中间文件', exact=True)).to_be_visible()
            expect(panel.get_by_text('最终交付', exact=True)).to_be_visible()
            page.screenshot(path=str(output / f'{name}-expanded.png'))
            files.click()
            expect(panel).to_have_class('task-summary-panel is-collapsed')
            expect(plan).to_be_visible()
            expect(files).to_be_visible()
            assert_vertical_icons(panel)
            assert abs(panel.bounding_box()['width'] - 44) < 1
            assert plan.inner_text() == files.inner_text() == ''
            expect(panel.get_by_role('tabpanel')).to_have_count(0)
            expect(panel.locator('.task-summary-title')).to_be_hidden()
            page.screenshot(path=str(output / f'{name}-collapsed.png'))
            plan.click()
            expect(panel.get_by_role('tabpanel', name='任务规划')).to_be_visible()
            expect(plan).to_have_attribute('aria-selected', 'true')
            plan.click()
            files.click()
            expect(panel.get_by_role('tabpanel', name='交付文件')).to_be_visible()
            files.click()
            page.reload()
            page.wait_for_load_state('networkidle')
            expect(panel).to_have_class('task-summary-panel is-collapsed')
            expect(files).to_be_visible()
            files.click()
            for width in [960, 390]:
                page.set_viewport_size({'width': width, 'height': 820})
                expect(plan).to_be_visible()
                expect(files).to_be_visible()
                assert_vertical_icons(panel)
                assert panel.evaluate('(el) => el.scrollWidth <= el.clientWidth + 1')
            assert not errors, (name, errors)
            context.close()
    finally:
        browser.close()
print('PASS: two-icon vertical rail without a separate toggle, navigation, persistence, responsive layout and unchanged document window.')
