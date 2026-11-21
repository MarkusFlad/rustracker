// Mandelbrot engine for the music view, taken from our real-time zoomer:
//  * GPU: WebGL2 fragment shader, fp32 or df64 (double-float) depending on zoom depth
//  * Rust/WASM: the SIMD kernel (kernel/mandel_wasm.rs: 2 doubles per vector, 8 pixels interleaved) in Web Workers
// New compared with the zoomer: rotation, palette shift and flash for the music sync, and
// reprojection of the last finished frame so motion stays smooth while the CPU engine renders.
(function () {
'use strict';


// ---------------------------------------------------------------- palettes
const PAL_LEN = 2048, DENSITY = 16;
const PALETTES = [
  [[0,'#000764'],[0.16,'#206bcb'],[0.42,'#edffff'],[0.6425,'#ffaa00'],[0.8575,'#000200'],[1,'#000764']],
  [[0,'#0b0420'],[0.2,'#5a1a8c'],[0.45,'#ff4f7b'],[0.62,'#ffd36b'],[0.8,'#2fd3c6'],[1,'#0b0420']],
  [[0,'#000000'],[0.3,'#3a3a3a'],[0.55,'#e8e8e8'],[0.75,'#8a8a8a'],[1,'#000000']],
];
/** '#rrggbb' -> [r, g, b]. */
function hex(h) { return [parseInt(h.slice(1,3),16), parseInt(h.slice(3,5),16), parseInt(h.slice(5,7),16)]; }
/** PAL_LEN RGBA entries from colour stops [[t, '#rrggbb'], ...], smoothstep between stops. */
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
/** The palette rotated by `shift` entries (for the CPU engine, which bakes colours in). */
function rotated(p, shift) {
  const s = ((Math.round(shift) % PAL_LEN) + PAL_LEN) % PAL_LEN, o = new Uint32Array(PAL_LEN);
  for (let i = 0; i < PAL_LEN; i++) o[i] = p[(i + s) % PAL_LEN];
  return o;
}

// ---------------------------------------------------------------- CPU workers (Rust/WASM SIMD kernel)
/** Worker source: keeps a palette in kernel memory and renders row bands to RGBA. */
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
/** Full-screen triangle. */
const VS = `#version 300 es
in vec2 p; void main(){gl_Position=vec4(p,0.,1.);}`;
/** Escape time per pixel, fp32 or df64 (double-float: value + error term) arithmetic. */
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
/** A double as two floats (hi, lo) for the df64 shader. */
const split = (x) => { const hi = Math.fround(x); return [hi, Math.fround(x - hi)]; };

/**
 * Mandel(canvas): draws views {cx, cy, size, rot, shift, flash} onto a 2D canvas.
 * size = visible height in the complex plane, rot = rotation in radians,
 * shift = palette offset (palette entries), flash = 0..1 white overlay.
 * Engines: 'gpu' (WebGL2) and 'wasm' (SIMD kernel in Web Workers).
 */
function Mandel(canvas) {
  const ctx = canvas.getContext('2d');
  let palIdx = 0, basePal = makePalette(PALETTES[0]);
  let engine = 'gpu', gl = null, glCanvas = null, glU = {}, glPalTex = null;
  let gpuFactor = 1.5, lastMs = 0, lastInfo = '';
  const OVERSCAN = 1.3; // render a little larger than the view so rotation never shows corners

  // ---- GPU
  /** Compiles the shader and sets up the palette texture; false without WebGL2. */
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
  /** Uploads the current palette to the GPU. */
  function gpuPalette() {
    if (!gl) return;
    gl.bindTexture(gl.TEXTURE_2D, glPalTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, PAL_LEN, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(basePal.buffer));
  }
  /** True when the pixel step is too fine for fp32 at this position. */
  function needDf(step, v) { return step < 4e-7 * Math.max(1, Math.abs(v.cx), Math.abs(v.cy)); }

  /** Renders W x H pixels (with overscan) on the GPU, adapting the resolution to ~14 ms. */
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
  /** Compiles the SIMD kernel and starts the worker pool; false without WASM SIMD. */
  async function initCPU() {
    try {
      // the SIMD kernel, built from kernel/mandel_wasm.rs
      wasmModule = await WebAssembly.compile(await (await fetch('mandel.wasm')).arrayBuffer());
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
  /** Sends the palette rotated by `shift` to all workers. */
  function sendPalette(shift) {
    const p = rotated(basePal, shift);
    for (const w of workers) w.postMessage({ type: 'pal', pal: p });
    palShiftSent = shift; palSentAt = performance.now();
  }
  /** Starts a new back buffer for view `v`, split into row bands for the workers. */
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
  /** Hands queued row bands to idle workers. */
  function dispatch(mi) {
    while (idle.length && queue.length) {
      const w = idle.pop(), j = queue.shift(), b = back;
      w.postMessage({ gen: b.gen, y0: j.y0, rows: j.rows, stride: b.w, re0: b.re0, im0: b.im0 - j.y0 * b.step,
        step: b.step, maxIter: mi, density: DENSITY });
    }
  }
  let curMi = 256;
  /** A band is done: copy it into the back buffer; when complete, swap and adapt (~40 ms). */
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

  /** Maps a finished buffer into the rotated, zoomed view (reprojection keeps motion smooth). */
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

  /** Iteration limit for the zoom depth (multiple of 4 for the SIMD kernel). */
  function maxIter(v) {
    const z = Math.max(1, 3 / v.size), l = Math.log2(z);
    return Math.round(Math.min(6000, Math.max(64, 120 + 60 * Math.pow(l, 1.15))) / 4) * 4;
  }

  /** GPU: render at the adaptive resolution and show it at once. */
  function drawGPU(v, cssW, cssH, dpr, mi) {
    const H = Math.max(8, Math.ceil(cssH * dpr * OVERSCAN / gpuFactor));
    const W = Math.max(8, Math.ceil(H * cssW / cssH));
    blit(renderGPU(v, W, H, mi), v, cssW, cssH, dpr);
  }
  /** CPU: start the next picture when the last one is done, show the newest finished one. */
  function drawCPU(v, cssW, cssH, dpr, mi) {
    // palette shift for the CPU engine: resend a rotated palette now and then
    const now = performance.now();
    if (Math.abs((v.shift || 0) - palShiftSent) > 8 && now - palSentAt > 90) sendPalette(v.shift || 0);
    if (!back) startCPU(v, cssH, dpr, mi);
    if (front) blit(front, v, cssW, cssH, dpr);
  }
  /** Additive white flash, strength 0..0.6. */
  function flashOverlay(flash) {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalCompositeOperation = 'lighter';
    ctx.fillStyle = 'rgba(255,255,255,' + Math.min(0.6, flash).toFixed(3) + ')';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.globalCompositeOperation = 'source-over';
  }

  return {
    /** Starts the engines; `{cpu: false}` skips the worker pool when WebGL2 works. */
    async init(opts) {
      const g = initGPU();
      // a GPU-only instance (e.g. a cube face) skips its own worker pool when WebGL2 works
      const c = (opts && opts.cpu === false && g) ? false : await initCPU();
      engine = g ? 'gpu' : 'wasm';
      return { gpu: g, wasm: c };
    },
    /** The available engines. */
    engines() { return [gl ? 'gpu' : null, cpuReady ? 'wasm' : null].filter(Boolean); },
    get engine() { return engine; },
    set engine(e) { engine = e; front = null; back = null; gen++; },
    /** Switches to the next palette. */
    nextPalette() {
      palIdx = (palIdx + 1) % PALETTES.length; basePal = makePalette(PALETTES[palIdx]);
      gpuPalette(); sendPalette(palShiftSent);
    },
    /** Engine, resolution and timing of the last picture, for the HUD. */
    get info() { return lastInfo; },
    /** Draws view `v` into a canvas of cssW x cssH CSS pixels at `dpr` device pixels each. */
    draw(v, cssW, cssH, dpr) {
      const mi = maxIter(v); curMi = mi;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = '#000'; ctx.fillRect(0, 0, canvas.width, canvas.height);
      if (engine === 'gpu' && gl) drawGPU(v, cssW, cssH, dpr, mi);
      else if (cpuReady) drawCPU(v, cssW, cssH, dpr, mi);
      if (v.flash > 0.01) flashOverlay(v.flash);
    },
  };
}

/** Interesting places for deep zooms: centre and smallest useful view height. */
Mandel.PLACES = [
  { name: 'Seepferdchental', cx: -0.743643887037151, cy: 0.131825904205330, min: 1e-4 },
  { name: 'Doppelspirale', cx: -0.77568377, cy: 0.13646737, min: 2e-6 },
  { name: 'Elefantental', cx: 0.2549870375144766, cy: -0.0005679790528465, min: 3e-6 },
  { name: 'Spirale', cx: -0.10109636384562, cy: 0.95628651080914, min: 2e-5 },
  { name: 'Mini-Mandelbrot', cx: -1.768778833, cy: -0.001738996, min: 4e-7 },
];
/** Palette helpers, shared with the voxel view (voxel.js). */
Mandel.makePalette = makePalette;
Mandel.PALETTES = PALETTES;
self.Mandel = Mandel;
})();
