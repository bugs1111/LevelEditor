// Execute installed original Heaps Blur.calcValues; no native kernel is imported.
const fs=require('fs'),crypto=require('crypto');
const source=fs.readFileSync(process.argv[2],'utf8');
const start=source.indexOf('h3d_pass_Blur.prototype =');
function method(name){const a=source.indexOf(','+name+': function(',start)+name.length+3;const b=source.indexOf('\n\t}',a)+3;return eval('('+source.slice(a,b)+')');}
const pass={radius:128,quality:1,gain:1,linear:0,gauss:method('gauss')};method('calcValues').call(pass);
const taps=pass.values.map((weight,i)=>[pass.offsets[i]*i,weight]);
fs.writeFileSync(__dirname+'/shadow-oracle.json',JSON.stringify({sha256:crypto.createHash('sha256').update(source).digest('hex'),radius:128,taps},null,2)+'\n');
console.log(taps);
