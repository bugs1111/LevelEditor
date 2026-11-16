const pages=await(await fetch('http://127.0.0.1:9347/json')).json();
const page=pages.find(p=>p.type==='page'&&p.url.includes('app.html'));
if(!page)throw Error('original app page not found');
const ws=new WebSocket(page.webSocketDebuggerUrl);let seq=0;const pending=new Map();let paused;
const stopped=new Promise(resolve=>paused=resolve);
ws.onmessage=({data})=>{const message=JSON.parse(data);if(message.method==='Debugger.paused')paused(message.params);if(message.id){const waiter=pending.get(message.id);pending.delete(message.id);message.error?waiter.reject(message.error):waiter.resolve(message.result);}};
await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
await call('Debugger.enable');await call('Debugger.pause');const state=await stopped;
try{
 for(const frame of state.callFrames){
  const result=await call('Debugger.evaluateOnCallFrame',{callFrameId:frame.callFrameId,expression:'typeof page_Editor!=="undefined" ? (window.__acceptanceEditor=page_Editor.ME,window.__acceptanceApp=App.ME,window.__acceptanceHome=page_Home,true) : false',returnByValue:true});
  if(result.result.value===true)break;
 }
}finally{await call('Debugger.resume');await call('Debugger.disable');}
const result=await call('Runtime.evaluate',{expression:'JSON.stringify((()=>{const e=window.__acceptanceEditor,a=window.__acceptanceApp,c=e?.camera;return {title:document.title,project:e?.project?.filePath,world:e?.worldMode,level:e?.curLevel?.identifier,levelSize:[e?.curLevel?.pxWid,e?.curLevel?.pxHei],camera:c?{worldX:c.worldX,worldY:c.worldY,rawZoom:c.rawZoom,pixelRatio:c._cachedPixelRatio,canvasWidth:c._cachedCanvasWidth,canvasHeight:c._cachedCanvasHeight}:null,size:[innerWidth,innerHeight,devicePixelRatio],canvases:[...document.querySelectorAll("canvas")].map(c=>({id:c.id,size:[c.width,c.height],rect:c.getBoundingClientRect().toJSON()})),text:document.body.innerText.slice(0,400)};})())',returnByValue:true});console.log(result.result.value);ws.close();
