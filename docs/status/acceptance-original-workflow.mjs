import {writeFile} from 'node:fs/promises';
const pages=await(await fetch('http://127.0.0.1:9347/json')).json();
const page=pages.find(p=>p.type==='page'&&p.url.includes('app.html'));
if(!page)throw new Error(JSON.stringify(pages.map(p=>({type:p.type,url:p.url}))));
const socket=new WebSocket(page.webSocketDebuggerUrl);let serial=0;const pending=new Map();
socket.onmessage=({data})=>{const value=JSON.parse(data);if(value.id&&pending.has(value.id)){const {resolve,reject}=pending.get(value.id);pending.delete(value.id);value.error?reject(value.error):resolve(value.result);}};
await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
function call(method,params={}){return new Promise((resolve,reject)=>{const id=++serial;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});}
await call('Page.bringToFront');
await call('Emulation.setFocusEmulationEnabled',{enabled:true});
await call('Emulation.setDeviceMetricsOverride',{width:1440,height:900,deviceScaleFactor:2,mobile:false});
await new Promise(r=>setTimeout(r,500));
await call('Runtime.evaluate',{expression:'(()=>{const c=window.__acceptanceEditor.camera;c.worldX=408;c.worldY=232;c.targetWorldX=null;c.targetWorldY=null;c.setZoom(2);})()'});
const actions=JSON.parse(process.argv[2]);
for(const a of actions){
 if(a.wait)await new Promise(r=>setTimeout(r,a.wait));
 if(a.eval)console.log(JSON.stringify(await call('Runtime.evaluate',{expression:a.eval,returnByValue:true})));
 if(a.pointer)await call('Input.dispatchMouseEvent',{type:a.button?(a.pressed?'mousePressed':'mouseReleased'):'mouseMoved',x:a.pointer[0],y:a.pointer[1],button:a.button==='secondary'?'right':a.button?'left':'none',buttons:a.buttons??(a.button&&a.pressed?1:0),clickCount:a.button?1:0,modifiers:(a.shift?8:0)|(a.alt?1:0)|(a.command?4:0)});
 if(a.key)await call('Input.dispatchKeyEvent',{type:a.pressed?'keyDown':'keyUp',key:a.key,code:a.key.length===1?'Key'+a.key.toUpperCase():a.key,windowsVirtualKeyCode:a.keyCode??a.key.toUpperCase().charCodeAt(0),modifiers:(a.shift?8:0)|(a.alt?1:0)|(a.command?4:0)});
}
await new Promise(r=>setTimeout(r,700));
const result=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
await writeFile(process.argv[3],Buffer.from(result.data,'base64'));
console.log('saved '+process.argv[3]);socket.close();
