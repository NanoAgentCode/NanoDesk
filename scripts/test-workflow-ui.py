"""UI smoke with mocked IPC. Run with Vite at http://localhost:1420."""
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

root = Path(__file__).resolve().parents[1]
entry = root / '.workflow-smoke.tsx'
html = root / '.workflow-smoke.html'
assert not entry.exists() and not html.exists()
try:
    entry.write_text('''import React from 'react';
import {createRoot} from 'react-dom/client';
import {MantineProvider} from '@mantine/core';
import '@mantine/core/styles.css';
import WorkflowPanel from './src/components/WorkflowPanel';
const notice = (s:string) => { document.getElementById('notice')!.textContent=s; };
createRoot(document.getElementById('root')!).render(<MantineProvider><WorkflowPanel setNotice={notice}/></MantineProvider>);
''', encoding='utf-8')
    html.write_text('''<html><body style="margin:0"><div id="root" style="height:100vh"></div><div id="notice"></div><script type="module" src="/.workflow-smoke.tsx"></script></body></html>''', encoding='utf-8')
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, channel='msedge')
        page = browser.new_page(viewport={'width': 1600, 'height': 1000})
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.add_init_script('''window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};window.__TAURI_INTERNALS__={
          callbacks:{}, next:0,
          transformCallback(fn){const id=++this.next;this.callbacks[id]=fn;return id},
          unregisterCallback(id){delete this.callbacks[id]},
          async invoke(cmd,args){
            if(cmd==='list_model_configs')return [{id:'model',name:'Test provider',model:'test-chat-model',model_kind:'chat'}];
            if(cmd==='list_agent_tool_definitions')return [{name:'read_file',description:'Read',parameters_json:'{}',risk:'low',requires_approval:false}];
            if(cmd==='list_mcp_servers')return [];
            if(cmd==='plugin:event|listen'){this.handler=args.handler;return args.handler}
            if(cmd==='chat_stream'){while(window.holdWorkflow)await new Promise(r=>setTimeout(r,20));this.callbacks[this.handler]({payload:{request_id:args.request.request_id,type:'delta',content:'AI answer'}});return;}
          }
        };''')
        page.goto('http://127.0.0.1:1420/.workflow-smoke.html')
        page.wait_for_load_state('networkidle')
        page.get_by_role('button', name='新建工作流').click()
        expect(page.get_by_role('textbox', name='默认模型', exact=True)).to_have_value('test-chat-model - Test provider')
        page.get_by_role('button', name='＋ AI 提示词', exact=True).click()
        page.get_by_role('textbox', name='模型', exact=True).click()
        page.get_by_role('option', name='test-chat-model - Test provider', exact=True).click()
        expect(page.get_by_role('textbox', name='模型', exact=True)).to_have_value('test-chat-model - Test provider')
        page.get_by_label('提示词', exact=True).fill('Write a plan')
        first_id = page.locator('.workflow-inspector code').inner_text()
        page.get_by_label('名称', exact=True).fill('需求分析')
        page.get_by_role('button', name='＋ 汇合', exact=True).click()
        page.get_by_label('名称', exact=True).fill('汇总')
        page.get_by_role('textbox', name='连线起点').click()
        page.get_by_role('option', name=f'需求分析 · {first_id}').click()
        second_id = page.locator('.workflow-inspector code').inner_text()
        page.get_by_role('textbox', name='连线终点').click()
        page.get_by_role('option', name=f'汇总 · {second_id}').click()
        page.get_by_role('button', name='连接', exact=True).click()
        expect(page.locator('.workflow-edges path')).to_have_count(1)
        page.get_by_role('button', name='运行全部', exact=True).click()
        expect(page.locator('.workflow-node.success')).to_have_count(2)
        expect(page.locator('.workflow-inspector')).to_contain_text('AI answer')
        heading = page.locator('.workflow-node-heading').first
        box = heading.bounding_box()
        page.mouse.move(box['x']+60, box['y']+15)
        page.mouse.down()
        page.mouse.move(box['x']+140, box['y']+80, steps=5)
        page.mouse.up()
        page.reload()
        page.wait_for_load_state('networkidle')
        expect(page.locator('.workflow-node')).to_have_count(2)
        expect(page.locator('.workflow-edges path')).to_have_count(1)
        expect(page.locator('.workflow-node.success')).to_have_count(2)
        page.locator('.workflow-node').first.click()
        expect(page.get_by_label('提示词', exact=True)).to_have_value('Write a plan')
        page.get_by_label('提示词', exact=True).fill('Updated plan')
        page.locator('.workflow-node').last.get_by_role('button', name='运行节点').click()
        expect(page.locator('#notice')).to_contain_text('先运行上游')
        page.evaluate('window.holdWorkflow=true')
        page.get_by_role('button', name='运行全部', exact=True).click()
        for title in ['AI 提示词', '工具', '条件', '汇合']:
            palette = page.get_by_role('button', name=f'＋ {title}', exact=True)
            expect(palette).to_be_disabled()
            expect(palette).to_have_attribute('draggable', 'false')
        page.evaluate('window.holdWorkflow=false')
        expect(page.get_by_role('button', name='运行全部', exact=True)).to_be_enabled()
        # Palette drag creates exactly one selected node at the drop position,
        # including transformed coordinates after zoom and scrolling.
        viewport = page.locator('.workflow-viewport')
        for index, title in enumerate(['AI 提示词', '工具', '条件', '汇合']):
            if index == 1:
                page.get_by_role('button', name='−', exact=True).click()
                page.get_by_role('button', name='−', exact=True).click()
                viewport.evaluate('(el) => { el.scrollLeft=220; el.scrollTop=140; }')
            scale = 1 if index == 0 else .8
            scroll = viewport.evaluate('(el) => ({x:el.scrollLeft,y:el.scrollTop})')
            page.get_by_role('button', name=f'＋ {title}', exact=True).drag_to(viewport, target_position={'x': 600, 'y': 280})
            expect(page.locator('.workflow-node')).to_have_count(3 + index)
            created = page.locator('.workflow-node').last
            expect(created).to_have_class(__import__('re').compile('selected'))
            coordinates = created.evaluate('(el) => ({x:parseFloat(el.style.left),y:parseFloat(el.style.top)})')
            assert abs(coordinates['x'] - (600 + scroll['x']) / scale) < 3, coordinates
            assert abs(coordinates['y'] - (280 + scroll['y']) / scale) < 3, coordinates
            expect(page.locator('.workflow-drop-preview')).to_have_count(0)
        page.reload()
        page.wait_for_load_state('networkidle')
        expect(page.locator('.workflow-node')).to_have_count(6)
        output = root / 'output' / 'workflow'
        output.mkdir(parents=True, exist_ok=True)
        page.screenshot(path=str(output / 'canvas.png'), full_page=True)
        assert not errors, errors
        browser.close()
        print('PASS: palette drag for four node types, zoom/scroll drop coordinates, running lock, persistence, model labels and existing canvas interactions; no browser errors')
finally:
    entry.unlink(missing_ok=True)
    html.unlink(missing_ok=True)
