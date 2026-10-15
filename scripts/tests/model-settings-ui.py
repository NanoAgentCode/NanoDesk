from playwright.sync_api import sync_playwright, expect
MOCK = """
window.suppliers=['A','B'].map(id=>({id,name:id,provider:'openai-compatible',base_url:'http://'+id,api_key:'test',created_at:'',updated_at:''}));
window.models=[]; window.calls=[]; window.pending={};
window.__TAURI_INTERNALS__={invoke:async(command,args={})=>{
 window.calls.push({command,args});
 if(command==='list_model_configs') return window.models;
 if(command==='list_model_suppliers') return window.suppliers;
 if(command==='get_asr_config') return null;
 if(command==='list_available_models') return new Promise((resolve,reject)=>{ window.pending[args.draft.name]={resolve,reject}; });
 if(command==='save_model_config') {const saved={...args.draft,id:args.draft.id||'model-'+window.models.length}; window.models.push(saved);return saved;}
 if(command==='save_asr_config') return args.config;
 return null;
}};
"""
with sync_playwright() as p:
    browser=p.chromium.launch(channel='msedge',headless=True)
    page=browser.new_page(viewport={'width':1100,'height':760})
    page.add_init_script(MOCK)
    page.goto('http://127.0.0.1:1420/scripts/tests/model-settings.html')
    page.wait_for_load_state('networkidle')
    a=page.get_by_role('button',name='获取 A 模型列表')
    b=page.get_by_role('button',name='获取 B 模型列表')
    a.click(); b.click()
    expect(a).to_be_disabled()
    expect(b).to_be_disabled()
    page.evaluate("window.pending.A.resolve([{id:'gpt-test',suggested_kind:'chat',context_window:128000}])")
    expect(a).to_be_enabled()
    expect(b).to_be_disabled()
    page.evaluate("window.pending.B.reject('connection failed')")
    expect(b).to_be_enabled()
    assert page.evaluate("window.model.supplierModels.A[0].id")=='gpt-test'
    b.click()
    expect(b).to_be_disabled()
    page.evaluate("window.pending.B.resolve([])")
    expect(b).to_be_enabled()
    # The extracted supplier hook creates models and preserves model metadata.
    saved=page.evaluate("async()=>await window.model.ensureSupplierModel('A',{id:'gpt-test',suggested_kind:'chat',context_window:128000})")
    assert saved['context_window']==128000
    again=page.evaluate("async()=>await window.model.ensureSupplierModel('A',{id:'gpt-test',suggested_kind:'chat'})")
    assert saved['id']==again['id']
    assert page.evaluate("window.calls.filter(c=>c.command==='save_model_config').length")==1
    asr=page.evaluate("async()=>await window.model.ensureSupplierModel('A',{id:'whisper-1',suggested_kind:'chat'})")
    assert asr['model_kind']=='asr'
    page.evaluate("async(id)=>await window.model.handleSelectAsrModel(id,'A')",asr['id'])
    page.wait_for_function("window.model.asrConfig?.model==='whisper-1'")
    assert page.evaluate("window.model.asrConfig.supplier_id")=='A'
    assert page.evaluate("window.model.asrConfig.api_key")==''
    page.evaluate("async()=>await window.model.handleSaveAsrLanguage('zh')")
    page.wait_for_function("window.model.asrConfig?.language==='zh'")
    rejected=page.evaluate("async(id)=>{try {await window.model.handleSelectAsrModel(id,'A');return false;}catch{return true;}}",saved['id'])
    assert rejected
    embedding=page.evaluate("async()=>await window.model.ensureSupplierModel('A',{id:'text-embedding-3-small',suggested_kind:'embedding'})")
    page.evaluate("async(id)=>await window.model.handleSelectEmbeddingModel(id)",embedding['id'])
    page.wait_for_function("window.model.embeddingDraft.embedding_model==='text-embedding-3-small'")
    assert page.evaluate("window.model.embeddingDraft.id")=='embedding-config'
    page.evaluate("async()=>await window.model.handleTestEmbedding()")
    page.wait_for_function("window.model.embeddingTestStatus.status==='success'")
    page.evaluate("async(id)=>await window.model.handleSelectEmbeddingModel(id)",saved['id'])
    assert page.evaluate("window.notice")=='请选择支持嵌入用途的供应商模型'
    print('PASS: concurrent supplier refresh/failure/retry, model reuse, ASR inference/selection/language, embedding selection/test and invalid-purpose rejection')
    browser.close()
