from pathlib import Path
from playwright.sync_api import sync_playwright, expect
root=Path(__file__).resolve().parents[2]/".build"/"refactor-check"
root.mkdir(parents=True, exist_ok=True)
mock = r"""
window.fixtures = [
 {id:'old',title:'全球科技市场简报',archived:false,model_config_id:'model',updated_at:'2026-10-14',project_path:null},
 {id:'project',title:'项目进度',archived:false,model_config_id:'model',updated_at:'2026-10-13',project_path:'D:/project'},
 {id:'archive',title:'归档记录',archived:true,updated_at:'2026-10-12'}
];
const label = location.search.includes('assistant') ? 'assistant' : 'main';
const bus = new BroadcastChannel('assistant-smoke');
const callbacks = new Map(), listeners = new Map(); let next = 1;
bus.onmessage = ({data}) => {
 if(data.target !== label) return;
 for(const [id,item] of listeners) if(item.event === data.event) callbacks.get(item.handler)?.({id,event:data.event,payload:data.payload});
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (_,id) => listeners.delete(id)};
window.__TAURI_INTERNALS__ = {
 metadata: {currentWindow:{label},currentWebview:{label}},
 transformCallback: callback => {const id=next++; callbacks.set(id,callback); return id;},
 unregisterCallback: id => callbacks.delete(id),
 invoke: async (command,args={}) => {
  if(command === 'plugin:event|listen'){ const id=next++; listeners.set(id,args); return id; }
  if(command === 'plugin:event|emit_to'){ bus.postMessage({target:args.target.label,event:args.event,payload:args.payload}); return; }
  if(command === 'list_conversation_project_paths') return ['D:/project'];
  if(command === 'list_conversations') return window.fixtures.filter(c => !c.archived && (c.project_path||null) === (args.projectPath||null));
  if(command === 'resize_assistant') { window.presentation = args.presentation; return; }
  if(command === 'plugin:dialog|open') return ['D:/test.pdf'];
  if(command === 'read_assistant_clipboard_paths') return window.clipboardPaths || [];
  return null;
 }
};
"""
mock=mock.replace("return null;", "if(command==='plugin:window|is_focused') return true; if(command==='list_background_agents') return []; return null;")
mock+="""
window.testEmit=(event,payload)=>{for(const [id,item] of listeners) if(item.event===event) callbacks.get(item.handler)?.({id,event,payload});};
const invoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
 if(command==='plugin:event|emit_to' && args.event==='assistant:request') {
   window.lastRequest=args.payload;
   if(window.deferSubmit)return;
   const files=[...(args.payload.filePaths||[]),...(args.payload.clipboardFiles||[])];
   files.forEach((_,index)=>window.testEmit('assistant:reply',{id:args.payload.id,attachment:{index,status:window.failSubmit?'failed':'success',...(window.failSubmit?{error:'测试文件无法读取'}:{})}}));
   window.testEmit('assistant:reply',{id:args.payload.id,...(window.failSubmit?{error:'模拟提交失败'}:{})});return;
 }
 return invoke(command,args);
};
"""
with sync_playwright() as p:
    browser=p.chromium.launch(channel='msedge',headless=True)
    for scale in [1,1.5,2]:
        context=browser.new_context(viewport={'width':88,'height':88},device_scale_factor=scale,permissions=['clipboard-read','clipboard-write'])
        context.add_init_script(mock)
        page=context.new_page(); errors=[]
        page.on('pageerror',lambda error:errors.append(str(error)))
        page.goto('http://127.0.0.1:1420/?assistant')
        page.wait_for_load_state('networkidle')
        orb=page.get_by_role('button',name='打开桌面助手')
        orb.evaluate("e=>e.dataset.identity='persistent'")
        expect(orb).to_be_visible()
        box=orb.bounding_box()
        assert box['width']==56 and box['x']==16 and box['y']==16,box
        orb.click()
        page.set_viewport_size({'width':532,'height':220})
        field=page.get_by_role('combobox',name='助手消息')
        expect(field).to_be_focused()
        page.wait_for_timeout(250)
        expect(orb).to_have_attribute('data-identity','persistent')
        panel=page.locator('#assistant-panel').bounding_box(); icon=orb.bounding_box()
        assert panel['y']>=0 and panel['y']+panel['height']<=icon['y']-8,(panel,icon)
        assert panel['x']>=0 and panel['x']+panel['width']<=532,(panel,icon)
        assert page.locator('.assistant-toolbar').count()==0
        field.fill('@科技')
        page.set_viewport_size({'width':532,'height':420})
        expect(page.get_by_role('option')).to_have_count(1)
        panel=page.locator('#assistant-panel').bounding_box(); icon=orb.bounding_box()
        assert panel['y']>=0 and panel['y']+panel['height']<=icon['y']-8,(panel,icon)
        field.press('Enter')
        page.set_viewport_size({'width':532,'height':220})
        expect(page.get_by_title('继续：全球科技市场简报；点击取消')).to_be_visible()
        field.fill('继续分析')
        # The persistent icon toggles only the composer and preserves the draft.
        orb.click()
        expect(orb).to_have_attribute('aria-expanded','false')
        expect(page.locator('#assistant-panel')).to_have_count(0)
        expect(orb).to_have_attribute('data-identity','persistent')
        orb.click()
        expect(field).to_be_focused()
        expect(field).to_have_value('继续分析')
        orb.dblclick()
        expect(orb).to_have_attribute('aria-expanded','true')
        expect(field).to_be_focused()
        expect(field).to_have_value('继续分析')
        page.screenshot(path=str(root/f'assistant-open-{scale}.png'),omit_background=True)
        field.press('Escape')
        expect(page.locator('#assistant-panel')).to_have_count(0)
        page.set_viewport_size({'width':88,'height':88})
        expect(orb).to_have_attribute('data-identity','persistent')
        orb.click(); page.set_viewport_size({'width':532,'height':220})
        expect(field).to_have_value('继续分析')
        if scale==1:
            # Refocus cancels blur; a subsequent five-second blur hides only the input.
            page.evaluate("window.testEmit('tauri://blur',null)")
            page.wait_for_timeout(3000)
            page.evaluate("window.testEmit('tauri://focus',null)")
            page.wait_for_timeout(2300)
            expect(field).to_be_visible()
            page.evaluate("window.testEmit('tauri://blur',null)")
            expect(page.locator('#assistant-panel')).to_have_count(0,timeout=6500)
            expect(orb).to_be_visible()
            orb.click(); expect(field).to_have_value('继续分析')
            snapshot={'run_id':'run','conversation_id':'old','status':'running','stream_message':None,'reasoning':'','executing_tool_message_id':None,'error':None}
            page.evaluate("s=>window.testEmit('background-agent',s)",snapshot)
            expect(orb).to_have_attribute('aria-busy','true')
            expect(field).to_be_visible()
            snapshot['status']='completed'
            page.evaluate("s=>window.testEmit('background-agent',s)",snapshot)
            expect(orb).to_have_attribute('aria-busy','false')
        page.evaluate('window.failSubmit=true')
        field.press('Enter')
        expect(page.get_by_role('button',name='发送消息')).to_have_attribute('title','模拟提交失败')
        expect(page.get_by_role('alert')).to_have_count(0)
        expect(field).to_have_value('继续分析')
        page.evaluate('window.failSubmit=false')
        # Real text paste keeps the cursor position and native insertion behavior.
        field.fill('前后')
        field.evaluate('e=>e.setSelectionRange(1,1)')
        page.evaluate("navigator.clipboard.writeText('粘贴')")
        field.press('Control+v')
        expect(field).to_have_value('前粘贴后')
        # Explorer fallback also works when no browser paste event is dispatched.
        page.evaluate("window.clipboardPaths=['D:/paste.pdf']")
        field.dispatch_event('keydown',{'key':'v','ctrlKey':True})
        expect(page.get_by_role('button',name='移除文件 paste.pdf')).to_be_visible()
        file_box=page.get_by_role('button',name='移除文件 paste.pdf').bounding_box()
        input_box=page.locator('.assistant-input-bar').bounding_box()
        assert file_box['y']+file_box['height']<=input_box['y'],(file_box,input_box)
        assert page.locator('#assistant-panel').evaluate('e=>e.scrollHeight<=e.clientHeight'), 'assistant panel must not scroll vertically'
        page.get_by_role('button',name='移除文件 paste.pdf').click()
        page.evaluate('window.clipboardPaths=[]')
        # Clipboard blobs survive submission failures and reach the main window.
        field.evaluate("""e=>{const data=new DataTransfer();data.items.add(new File(['hello'],'clip.txt',{type:'text/plain'}));e.dispatchEvent(new ClipboardEvent('paste',{clipboardData:data,bubbles:true,cancelable:true}));}""")
        expect(page.get_by_role('button',name='移除文件 clip.txt')).to_be_visible()
        # Picked and pasted files share one row above the input, even with long names.
        page.get_by_role('button',name='上传文件').click()
        page.evaluate("window.clipboardPaths=['D:/a-very-long-attachment-name-for-layout-check.pdf','D:/another-long-attachment-name.pdf']")
        field.dispatch_event('keydown',{'key':'v','ctrlKey':True})
        expect(page.get_by_role('button',name='移除文件 another-long-attachment-name.pdf')).to_be_attached()
        assert page.locator('.assistant-files').count()==1
        assert page.locator('#assistant-panel').evaluate('e=>e.scrollHeight<=e.clientHeight')
        page.get_by_role('button',name='移除文件 another-long-attachment-name.pdf').scroll_into_view_if_needed()
        for chip in page.locator('.assistant-files button').all():
            box=chip.bounding_box(); bar=page.locator('.assistant-input-bar').bounding_box()
            assert box['y']>=0 and box['y']+box['height']<=bar['y'],(box,bar)
        page.evaluate('window.deferSubmit=true')
        page.get_by_role('button',name='发送消息').click()
        expect(page.get_by_role('button',name='发送消息')).to_be_disabled()
        page.evaluate("""()=>{const id=window.lastRequest.id;for(const [index,status] of [[0,'success'],[1,'failed'],[2,'processing'],[3,'processing']])window.testEmit('assistant:reply',{id,attachment:{index,status,...(status==='failed'?{error:'文件无法读取'}:{})}});}""")
        expect(page.locator('.assistant-file-status--success')).to_have_count(1)
        expect(page.locator('.assistant-file-status--failed')).to_have_count(1)
        expect(page.locator('.assistant-file-status--processing')).to_have_count(2)
        expect(page.get_by_role('alert')).to_have_count(0)
        page.screenshot(path=str(root/f'assistant-file-status-{scale}.png'),omit_background=True)
        page.evaluate("""()=>{const id=window.lastRequest.id;for(const index of [2,3])window.testEmit('assistant:reply',{id,attachment:{index,status:'success'}});window.testEmit('assistant:reply',{id,error:'文件处理未完成'});window.deferSubmit=false;}""")
        expect(page.get_by_role('button',name='发送消息')).to_be_enabled()
        expect(page.locator('.assistant-file-status--success')).to_have_count(3)
        expect(page.locator('.assistant-file-status--failed')).to_have_count(1)
        for name in ['test.pdf','a-very-long-attachment-name-for-layout-check.pdf','another-long-attachment-name.pdf']:
            page.get_by_role('button',name=f'移除文件 {name}',exact=True).click()
        page.evaluate('window.clipboardPaths=[]')
        field.fill('')
        page.evaluate('window.failSubmit=true')
        page.get_by_role('button',name='发送消息').click()
        expect(page.locator('.assistant-file-status--failed')).to_have_count(1)
        expect(page.get_by_role('alert')).to_have_count(0)
        expect(page.get_by_role('button',name='移除文件 clip.txt')).to_be_visible()
        assert page.evaluate('window.lastRequest.clipboardFiles[0].name')=='clip.txt'
        assert page.evaluate("atob(window.lastRequest.clipboardFiles[0].dataUrl.split(',')[1])")=='hello'
        page.screenshot(path=str(root/f'assistant-clipboard-{scale}.png'),omit_background=True)
        page.get_by_role('button',name='移除文件 clip.txt').click()
        page.evaluate('window.failSubmit=false')
        field.fill('继续分析')
        page.get_by_role('button',name='上传文件').click()
        expect(page.get_by_role('button',name='移除文件 test.pdf')).to_be_visible()
        panel=page.locator('#assistant-panel').bounding_box(); icon=orb.bounding_box()
        assert panel['y']>=0 and panel['y']+panel['height']<=icon['y']-8,(panel,icon)
        field.press('Enter')
        expect(page.locator('#assistant-panel')).to_have_count(0)
        expect(orb).to_have_attribute('data-identity','persistent')
        assert page.evaluate('window.lastRequest.conversationId')=='old'
        assert page.evaluate('window.lastRequest.filePaths')==['D:/test.pdf']
        assert not errors, errors
        print(f'PASS scale={scale}: persistent icon, composer above icon, picker, focus, Esc, draft, error, attachment and submit')
        context.close()
    browser.close()
