// Mandelbrot engine for the music view, taken from our real-time zoomer:
//  * GPU: WebGL2 fragment shader, fp32 or df64 (double-float) depending on zoom depth
//  * Rust/WASM: the SIMD kernel (2 doubles per vector, 8 pixels interleaved) in Web Workers
// New compared with the zoomer: rotation, palette shift and flash for the music sync, and
// reprojection of the last finished frame so motion stays smooth while the CPU engine renders.
(function () {
'use strict';

const WASM_B64 = "AGFzbQEAAAABDwFgC39/f3x8fH9/f3x8AAMCAQAFAwEAEAYJAX8BQYCAwAALBxMCBm1lbW9yeQIABnJlbmRlcgAACrsmAbgmDQF/AX4BfwF8AX8BewF/InsBfwF+AX8BfgF8AkAgAkUNACABRQ0AIAhBf2ohCyAGrSEMQQAhDUQAAAAAAAAAACEOA0AgDSABbCEPIAQgBSAOoqH9FCEQQQAhEQNAIAMgBSARuKKg/RQgAyAFIBFBAXK4oqD9IgEhEiADIAUgEUEGcriioP0UIAMgBSARQQdyuKKg/SIBIRMgAyAFIBFBBHK4oqD9FCADIAUgEUEFcriioP0iASEUIAMgBSARQQJyuKKg/RQgAyAFIBFBA3K4oqD9IgEhFQJAAkAgBg0A/QwAAAAAAAAAAAAAAAAAAAAAIhYhFyAWIRggFiEZIBAhGiAQIRsgECEcIBAhHSATIR4gFCEfIBUhICASISEMAQtBBCEIIBIhISAVISAgFCEfIBMhHiAQIR0gECEcIBAhGyAQIRr9DAAAAAAAAAAAAAAAAAAAAAAiGSEYIBkhFyAZIRYDQCAQIBAgECAQIB4gHv3wASAa/fIB/fABIBogHiAe/fIBIiIgGiAa/fIBIiP98AH9DAAAAAAAABBAAAAAAAAAEEAiJP1LIiX9UiIaIBMgIiAj/fEB/fABIB4gJf1SIh4gHv3wAf3yAf3wASAaIBogGv3yASIiIB4gHv3yASIj/fABICT9SyIm/VIiGiATICMgIv3xAf3wASAeICb9UiIeIB798AH98gH98AEgGiAaIBr98gEiIiAeIB798gEiI/3wASAk/UsiJ/1SIhogEyAjICL98QH98AEgHiAn/VIiHiAe/fAB/fIB/fABIBogGiAa/fIBIiMgHiAe/fIBIij98AEgJP1LIiL9UiEaIBMgKCAj/fEB/fABIB4gIv1SIR4gECAQIBAgECAfIB/98AEgG/3yAf3wASAbIB8gH/3yASIjIBsgG/3yASIo/fABICT9SyIp/VIiGyAUICMgKP3xAf3wASAfICn9UiIfIB/98AH98gH98AEgGyAbIBv98gEiIyAfIB/98gEiKP3wASAk/UsiKv1SIhsgFCAoICP98QH98AEgHyAq/VIiHyAf/fAB/fIB/fABIBsgGyAb/fIBIiMgHyAf/fIBIij98AEgJP1LIiv9UiIbIBQgKCAj/fEB/fABIB8gK/1SIh8gH/3wAf3yAf3wASAbIBsgG/3yASIoIB8gH/3yASIs/fABICT9SyIj/VIhGyAUICwgKP3xAf3wASAfICP9UiEfIBAgECAQIBAgICAg/fABIBz98gH98AEgHCAgICD98gEiKCAcIBz98gEiLf3wASAk/UsiLP1SIhwgFSAoIC398QH98AEgICAs/VIiICAg/fAB/fIB/fABIBwgHCAc/fIBIiggICAg/fIBIi798AEgJP1LIi39UiIcIBUgLiAo/fEB/fABICAgLf1SIiAgIP3wAf3yAf3wASAcIBwgHP3yASIoICAgIP3yASIv/fABICT9SyIu/VIiHCAVIC8gKP3xAf3wASAgIC79UiIgICD98AH98gH98AEgHCAcIBz98gEiLyAgICD98gEiMP3wASAk/UsiKP1SIRwgFSAwIC/98QH98AEgICAo/VIhICAQIBAgECAQICEgIf3wASAd/fIB/fABIB0gISAh/fIBIjAgHSAd/fIBIjH98AEgJP1LIi/9UiIdIBIgMCAx/fEB/fABICEgL/1SIiEgIf3wAf3yAf3wASAdIB0gHf3yASIxICEgIf3yASIy/fABICT9SyIw/VIiHSASIDIgMf3xAf3wASAhIDD9UiIhICH98AH98gH98AEgHSAdIB398gEiMiAhICH98gEiM/3wASAk/UsiMf1SIh0gEiAzIDL98QH98AEgISAx/VIiISAh/fAB/fIB/fABIB0gHSAd/fIBIjIgISAh/fIBIjP98AEgJP1LIiT9UiEdIBIgMyAy/fEB/fABICEgJP1SISEgFiAl/dEBICb90QEgJ/3RASAi/dEBIRYgFyAp/dEBICr90QEgK/3RASAj/dEBIRcgGCAs/dEBIC390QEgLv3RASAo/dEBIRggGSAv/dEBIDD90QEgMf3RASAk/dEBIRkgKCAk/VAgI/1QICL9UP1TRQ0BIAggBkkhNCAIQQRqIQggNA0ACwsgESAPakECdCAAaiEIIBn9HQEhNUGAgIB4ITRBgICAeCE2AkAgGf0dACI3IAxZDQAgCyAKIAlEAAAAAAAAAAAgN7lEAAAAAAAA8D+gIB0gHf3yASAhICH98gH98AH9HQAiN0L/////////B4NCgICAgICAgPg/hL8iOEQAAAAAAADwv6AgOEQAAAAAAADwP6CjIjgg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";

// ---------------------------------------------------------------- palettes
const PAL_LEN = 2048, DENSITY = 16;
const PALETTES = [
  [[0,'#000764'],[0.16,'#206bcb'],[0.42,'#edffff'],[0.6425,'#ffaa00'],[0.8575,'#000200'],[1,'#000764']],
  [[0,'#0b0420'],[0.2,'#5a1a8c'],[0.45,'#ff4f7b'],[0.62,'#ffd36b'],[0.8,'#2fd3c6'],[1,'#0b0420']],
  [[0,'#000000'],[0.3,'#3a3a3a'],[0.55,'#e8e8e8'],[0.75,'#8a8a8a'],[1,'#000000']],
];
function hex(h) { return [parseInt(h.slice(1,3),16), parseInt(h.slice(3,5),16), parseInt(h.slice(5,7),16)]; }
function makePalette(stops) {
  const p = new Uint32Array(PAL_LEN);
  for (let i = 0; i < PAL_LEN; i++) {
    const t = i / PAL_LEN; let k = 0; while (k < stops.length - 2 && stops[k+1][0] < t) k++;
    const [t0, c0] = stops[k], [t1, c1] = stops[k+1], a = hex(c0), b = hex(c1);
    let f = (t - t0) / (t1 - t0); f = f * f * (3 - 2 * f);
    const r = a[0] + (b[0]-a[0]) * f, g = a[1] + (b[1]-a[1]) * f, bl = a[2] + (b[2]-a[2]) * f;
    p[i] = (255 << 24 | (bl & 255) << 16 | (g & 255) << 8 | (r & 255)) >>> 0;
  }
  return p;
}
function rotated(p, shift) {
  const s = ((Math.round(shift) % PAL_LEN) + PAL_LEN) % PAL_LEN, o = new Uint32Array(PAL_LEN);
  for (let i = 0; i < PAL_LEN; i++) o[i] = p[(i + s) % PAL_LEN];
  return o;
}

// ---------------------------------------------------------------- CPU workers (Rust/WASM SIMD kernel)
const WORKER_SRC = `
let inst=null,mem=null,palPtr=0,palLen=0,outPtr=0,ready=null;
function ensure(bytes){const need=outPtr+bytes-mem.buffer.byteLength;if(need>0)mem.grow(Math.ceil(need/65536));}
function setPal(p){palLen=p.length;const v=new Uint32Array(mem.buffer,palPtr,palLen);v.set(p);}
onmessage=async e=>{
  const d=e.data;
  if(d.type==='init'){
    ready=(async()=>{
      inst=await WebAssembly.instantiate(d.module,{});
      mem=inst.exports.memory;
      palPtr=mem.buffer.byteLength; mem.grow(1); outPtr=palPtr+65536;
      setPal(d.pal);
    })();
    return;
  }
  await ready;
  if(d.type==='pal'){setPal(d.pal);return;}
  const bytes=d.stride*d.rows*4;
  ensure(bytes);
  inst.exports.render(outPtr,d.stride,d.rows,d.re0,d.im0,d.step,d.maxIter,palPtr,palLen,d.density,0);
  const buf=mem.buffer.slice(outPtr,outPtr+bytes);
  postMessage({gen:d.gen,y0:d.y0,rows:d.rows,buf},[buf]);
};`;

// ---------------------------------------------------------------- GPU (WebGL2)
const VS = `#version 300 es
in vec2 p; void main(){gl_Position=vec4(p,0.,1.);}`;
const FS = `#version 300 es
precision highp float; precision highp int;
uniform vec2 uRes; uniform vec2 uCx; uniform vec2 uCy; uniform float uStep;
uniform int uMaxIter; uniform int uDf; uniform float uDensity; uniform float uOne; uniform float uShift;
uniform sampler2D uPal;
out vec4 o;
vec2 qts(float a,float b){float s=a+b;float e=b-(s-a);return vec2(s,e);}
vec2 ts(float a,float b){float s=a+b;float v=(s*uOne)-a;float e=(a-(s-v))+(b-v);return vec2(s,e);}
vec2 spl(float a){float t=a*(4097.0*uOne);float hi=t-(t-a);return vec2(hi,a-hi);}
vec2 tp(float a,float b){float p=a*b;vec2 A=spl(a),B=spl(b);
  float e=((A.x*B.x-p)+A.x*B.y+A.y*B.x)+A.y*B.y;return vec2(p,e);}
vec2 dadd(vec2 a,vec2 b){vec2 s=ts(a.x,b.x);vec2 t=ts(a.y,b.y);s.y+=t.x;s=qts(s.x,s.y);s.y+=t.y;return qts(s.x,s.y);}
vec2 dmul(vec2 a,vec2 b){vec2 p=tp(a.x,b.x);p.y+=a.x*b.y+a.y*b.x;return qts(p.x,p.y);}
void main(){
  vec2 off=(gl_FragCoord.xy-uRes*0.5)*uStep;
  int n=0; float mag=0.0;
  if(uDf==0){
    vec2 c=vec2(uCx.x+off.x,uCy.x+off.y), z=c;
    for(int i=0;i<uMaxIter;i++){
      float x2=z.x*z.x,y2=z.y*z.y; mag=x2+y2; if(mag>4.0)break;
      z=vec2(x2-y2+c.x,2.0*z.x*z.y+c.y); n++;
    }
  }else{
    vec2 cr=dadd(uCx,vec2(off.x,0.0)), ci=dadd(uCy,vec2(off.y,0.0)), zr=cr, zi=ci;
    for(int i=0;i<uMaxIter;i++){
      vec2 x2=dmul(zr,zr), y2=dmul(zi,zi); mag=x2.x+y2.x; if(mag>4.0)break;
      vec2 xy=dmul(zr,zi);
      zi=dadd(dadd(xy,xy),ci); zr=dadd(dadd(x2,-y2),cr); n++;
    }
  }
  if(n>=uMaxIter){o=vec4(0.,0.,0.,1.);return;}
  float nu=max(0.0,float(n)+1.0-log2(log2(mag)*0.5));
  int idx=int(mod(floor(nu*uDensity+uShift),2048.0));
  o=texelFetch(uPal,ivec2(idx,0),0);
}`;
const split = (x) => { const hi = Math.fround(x); return [hi, Math.fround(x - hi)]; };

// Mandel(canvas): draws views {cx, cy, size, rot, shift, flash} onto a 2D canvas.
// size = visible height in the complex plane, rot = rotation in radians,
// shift = palette offset (palette entries), flash = 0..1 white overlay.
function Mandel(canvas) {
  const ctx = canvas.getContext('2d');
  let palIdx = 0, basePal = makePalette(PALETTES[0]);
  let engine = 'gpu', gl = null, glCanvas = null, glU = {}, glPalTex = null;
  let gpuFactor = 1.5, lastMs = 0, lastInfo = '';
  const OVERSCAN = 1.3; // render a little larger than the view so rotation never shows corners

  // ---- GPU
  function initGPU() {
    try {
      glCanvas = document.createElement('canvas');
      gl = glCanvas.getContext('webgl2', { antialias: false, preserveDrawingBuffer: true, powerPreference: 'high-performance' });
      if (!gl) return false;
      const sh = (type, src) => { const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
      const prog = gl.createProgram();
      gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS)); gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FS));
      gl.linkProgram(prog);
      if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
      gl.useProgram(prog);
      for (const u of ['uRes','uCx','uCy','uStep','uMaxIter','uDf','uDensity','uOne','uShift','uPal']) glU[u] = gl.getUniformLocation(prog, u);
      const buf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
      const loc = gl.getAttribLocation(prog, 'p'); gl.enableVertexAttribArray(loc); gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
      glPalTex = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, glPalTex);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gpuPalette();
      return true;
    } catch (e) { console.error('WebGL2:', e); gl = null; return false; }
  }
  function gpuPalette() {
    if (!gl) return;
    gl.bindTexture(gl.TEXTURE_2D, glPalTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, PAL_LEN, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(basePal.buffer));
  }
  function needDf(step, v) { return step < 4e-7 * Math.max(1, Math.abs(v.cx), Math.abs(v.cy)); }

  function renderGPU(v, W, H, mi) {
    const step = v.size * OVERSCAN / H;
    const df = needDf(step, v);
    if (glCanvas.width !== W || glCanvas.height !== H) { glCanvas.width = W; glCanvas.height = H; }
    const t0 = performance.now();
    gl.viewport(0, 0, W, H);
    gl.uniform2f(glU.uRes, W, H);
    gl.uniform2fv(glU.uCx, split(v.cx)); gl.uniform2fv(glU.uCy, split(v.cy));
    gl.uniform1f(glU.uStep, step); gl.uniform1i(glU.uMaxIter, mi); gl.uniform1i(glU.uDf, df ? 1 : 0);
    gl.uniform1f(glU.uDensity, DENSITY); gl.uniform1f(glU.uOne, 1); gl.uniform1f(glU.uShift, v.shift || 0);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, glPalTex); gl.uniform1i(glU.uPal, 0);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4)); // sync, for honest timing
    lastMs = performance.now() - t0;
    // adapt resolution so a frame takes ~14 ms
    const pf = gpuFactor * Math.sqrt(Math.max(lastMs, 0.5) / 14);
    gpuFactor = Math.max(1, Math.min(6, 0.7 * gpuFactor + 0.3 * pf));
    lastInfo = 'GPU (WebGL2, ' + (df ? 'df64' : 'fp32') + ') · 1/' + gpuFactor.toFixed(1) + ' · ' + lastMs.toFixed(1) + ' ms';
    return { c: glCanvas, re0: v.cx - (W / 2 - 0.5) * step, im0: v.cy + (H / 2 - 0.5) * step, step };
  }

  // ---- CPU (Rust/WASM in workers)
  let workers = [], idle = [], wasmModule = null, cpuReady = false;
  let gen = 0, queue = [], pending = 0, back = null, front = null, cpuFactor = 4, jobStart = 0;
  let palShiftSent = 0, palSentAt = 0;
  const NW = Math.max(2, Math.min(8, navigator.hardwareConcurrency || 4));
  async function initCPU() {
    try {
      const b = atob(WASM_B64), u = new Uint8Array(b.length);
      for (let i = 0; i < b.length; i++) u[i] = b.charCodeAt(i);
      wasmModule = await WebAssembly.compile(u);
      const url = URL.createObjectURL(new Blob([WORKER_SRC], { type: 'text/javascript' }));
      for (let i = 0; i < NW; i++) {
        const w = new Worker(url);
        w.onmessage = (e) => onResult(w, e.data);
        w.postMessage({ type: 'init', module: wasmModule, pal: basePal });
        workers.push(w); idle.push(w);
      }
      cpuReady = true;
      return true;
    } catch (e) { console.error('WASM SIMD:', e); return false; }
  }
  function sendPalette(shift) {
    const p = rotated(basePal, shift);
    for (const w of workers) w.postMessage({ type: 'pal', pal: p });
    palShiftSent = shift; palSentAt = performance.now();
  }
  function startCPU(v, cssH, dpr, mi) {
    gen++; queue = [];
    const H = Math.max(8, Math.ceil(cssH * dpr * OVERSCAN / cpuFactor));
    const W = Math.max(8, Math.ceil(H * canvas.width / canvas.height));
    const stride = Math.ceil(W / 8) * 8;
    const step = v.size * OVERSCAN / H;
    const c = document.createElement('canvas'); c.width = stride; c.height = H;
    back = { c, g: c.getContext('2d'), re0: v.cx - (stride / 2 - 0.5) * step, im0: v.cy + (H / 2 - 0.5) * step, step, gen, w: stride };
    const rows = Math.max(2, Math.ceil(H / (NW * 4)));
    for (let y = 0; y < H; y += rows) queue.push({ y0: y, rows: Math.min(rows, H - y) });
    pending = queue.length; jobStart = performance.now();
    dispatch(mi);
  }
  function dispatch(mi) {
    while (idle.length && queue.length) {
      const w = idle.pop(), j = queue.shift(), b = back;
      w.postMessage({ gen: b.gen, y0: j.y0, rows: j.rows, stride: b.w, re0: b.re0, im0: b.im0 - j.y0 * b.step,
        step: b.step, maxIter: mi, density: DENSITY });
    }
  }
  let curMi = 256;
  function onResult(w, d) {
    idle.push(w);
    if (back && d.gen === gen) {
      back.g.putImageData(new ImageData(new Uint8ClampedArray(d.buf), back.w, d.rows), 0, d.y0);
      if (--pending === 0) {
        lastMs = performance.now() - jobStart;
        front = back; back = null;
        const pf = cpuFactor * Math.sqrt(lastMs / 40); // aim at ~40 ms per frame
        cpuFactor = Math.max(1, Math.min(10, 0.5 * cpuFactor + 0.5 * pf));
        lastInfo = 'Rust/WASM SIMD · ' + NW + ' Threads · 1/' + cpuFactor.toFixed(1) + ' · ' + Math.round(lastMs) + ' ms';
      }
    }
    dispatch(curMi);
  }

  // ---- drawing: map a finished buffer into the rotated view
  function blit(b, v, cssW, cssH, dpr) {
    const s = v.size / cssH;                       // complex units per CSS pixel
    ctx.save();
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.translate(cssW / 2, cssH / 2);
    ctx.rotate(v.rot || 0);
    ctx.scale(b.step / s, b.step / s);
    ctx.translate((b.re0 - b.step / 2 - v.cx) / b.step, (v.cy - (b.im0 + b.step / 2)) / b.step);
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(b.c, 0, 0);
    ctx.restore();
  }

  function maxIter(v) {
    const z = Math.max(1, 3 / v.size), l = Math.log2(z);
    return Math.round(Math.min(6000, Math.max(64, 120 + 60 * Math.pow(l, 1.15))) / 4) * 4;
  }

  return {
    async init(opts) {
      const g = initGPU();
      // a GPU-only instance (e.g. a cube face) skips its own worker pool when WebGL2 works
      const c = (opts && opts.cpu === false && g) ? false : await initCPU();
      engine = g ? 'gpu' : 'wasm';
      return { gpu: g, wasm: c };
    },
    engines() { return [gl ? 'gpu' : null, cpuReady ? 'wasm' : null].filter(Boolean); },
    get engine() { return engine; },
    set engine(e) { engine = e; front = null; back = null; gen++; },
    nextPalette() {
      palIdx = (palIdx + 1) % PALETTES.length; basePal = makePalette(PALETTES[palIdx]);
      gpuPalette(); sendPalette(palShiftSent);
    },
    get info() { return lastInfo; },
    draw(v, cssW, cssH, dpr) {
      const mi = maxIter(v); curMi = mi;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = '#000'; ctx.fillRect(0, 0, canvas.width, canvas.height);
      if (engine === 'gpu' && gl) {
        const H = Math.max(8, Math.ceil(cssH * dpr * OVERSCAN / gpuFactor));
        const W = Math.max(8, Math.ceil(H * cssW / cssH));
        blit(renderGPU(v, W, H, mi), v, cssW, cssH, dpr);
      } else if (cpuReady) {
        // palette shift for the CPU engine: resend a rotated palette now and then
        const now = performance.now();
        if (Math.abs((v.shift || 0) - palShiftSent) > 8 && now - palSentAt > 90) sendPalette(v.shift || 0);
        if (!back) startCPU(v, cssH, dpr, mi);
        if (front) blit(front, v, cssW, cssH, dpr);
      }
      if (v.flash > 0.01) {
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.globalCompositeOperation = 'lighter';
        ctx.fillStyle = 'rgba(255,255,255,' + Math.min(0.6, v.flash).toFixed(3) + ')';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.globalCompositeOperation = 'source-over';
      }
    },
  };
}

// Interesting places for deep zooms: [cx, cy, smallest useful view height]
Mandel.PLACES = [
  { name: 'Seepferdchental', cx: -0.743643887037151, cy: 0.131825904205330, min: 1e-4 },
  { name: 'Doppelspirale', cx: -0.77568377, cy: 0.13646737, min: 2e-6 },
  { name: 'Elefantental', cx: 0.2549870375144766, cy: -0.0005679790528465, min: 3e-6 },
  { name: 'Spirale', cx: -0.10109636384562, cy: 0.95628651080914, min: 2e-5 },
  { name: 'Mini-Mandelbrot', cx: -1.768778833, cy: -0.001738996, min: 4e-7 },
];
self.Mandel = Mandel;
})();
