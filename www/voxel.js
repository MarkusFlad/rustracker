// Voxel view of the classic edition: the music director's Mandelbrot picture as a landscape.
//
// The SIMD kernel (kernel/mandel_wasm.rs, in Web Workers) renders a square picture of
// iteration indices. That becomes a height map: points of the set lie at height zero, the
// fewer iterations a point needs, the higher it is (histogram-equalised, so the heights
// spread evenly and the outer regions rise to peaks). Every voxel keeps the colour of the
// 2D picture, lit by its slope. A camera orbits the landscape and looks past its centre,
// so it flies around the peaks. Rendering is the classic voxel-space algorithm, one ray per
// screen column, into an ImageData at reduced resolution (the canvas is scaled up).
// Same algorithm as the framebuffer edition (src/vis_voxel.rs).
(function () {
'use strict';

const N = 320;              // height-map resolution (square, multiple of 8)
const OVERSCAN = 1.3;       // the picture covers 1.3 x the director's view
const HIST_BINS = 4096;     // one bin per iteration
const EXTENT = 1.6;         // half width of the landscape (the view spans ±1)
const HMAX = 0.55;          // world height of the highest voxels
const ZMAX = 2.6;           // farthest distance a ray travels
const MAX_W = 720;          // render width limit (pixels)
const BG = rgb(2, 5, 4), FOG = rgb(16, 34, 27), BLACK = rgb(0, 0, 0), WHITE = rgb(230, 245, 236);
const INSIDE = 0xffffffff;

/** Opaque colour as u32 (bytes R, G, B, A in memory, like ImageData). */
function rgb(r, g, b) { return (0xff000000 | (b << 16) | (g << 8) | r) >>> 0; }
/** Blend of colours a and b, t = weight of b (0..256). */
function mix(a, b, t) {
  const u = 256 - t;
  return rgb((((a & 255) * u + (b & 255) * t) >> 8), ((((a >>> 8) & 255) * u + ((b >>> 8) & 255) * t) >> 8),
    ((((a >>> 16) & 255) * u + ((b >>> 16) & 255) * t) >> 8));
}
/** Colour scaled in brightness, k = 0..256. */
function dim(c, k) { return rgb(((c & 255) * k) >> 8, (((c >>> 8) & 255) * k) >> 8, (((c >>> 16) & 255) * k) >> 8); }

/** Worker: renders row bands of iteration indices (16 per iteration, INSIDE for the set). */
const WSRC = `let ex, base, ready;
onmessage = async (e) => {
  const d = e.data;
  if (d.module) { ready = WebAssembly.instantiate(d.module, {}).then((i) => { ex = i.exports; base = ex.memory.buffer.byteLength; }); return; }
  await ready;
  const n = d.w * d.rows * 4, need = base + n - ex.memory.buffer.byteLength;
  if (need > 0) ex.memory.grow(Math.ceil(need / 65536));
  ex.render(base, d.w, d.rows, d.re0, d.im0, d.step, d.maxit, 0, 1 << 30, 16, 0);
  const buf = ex.memory.buffer.slice(base, base + n);
  postMessage({ gen: d.gen, y0: d.y0, rows: d.rows, buf }, [buf]);
};`;

/** Iteration limit for the zoom depth (as in mandel.js, a multiple of 4). */
function maxIter(v) {
  const l = Math.log2(Math.max(1, 3 / v.size));
  return Math.round(Math.min(6000, Math.max(64, 120 + 60 * Math.pow(l, 1.15))) / 4) * 4;
}

/**
 * Voxel(canvas): draws director views {cx, cy, size, shift, flash, m, rowF} as a landscape.
 * Call init() once; draw() every frame.
 */
function Voxel(canvas) {
  const ctx = canvas.getContext('2d');
  let palIdx = 0, pal = Mandel.makePalette(Mandel.PALETTES[0]);
  const workers = [];
  let gen = 0, pending = 0, back = null, front = null, lastMs = 0, jobStart = 0;
  const heights = new Uint16Array(N * N), light = new Uint8Array(N * N);
  const hist = new Uint32Array(HIST_BINS), cdf = new Uint32Array(HIST_BINS);
  let img = null, px = null, sky = null, t0 = performance.now(), pulse = 0, rowKey = '', lastT = 0;

  /** Starts a new picture of view `v`, split into row bands for the workers. */
  function start(v) {
    gen++;
    const step = v.size * OVERSCAN / N;
    back = { buf: new Uint32Array(N * N), re0: v.cx - (N / 2 - 0.5) * step, im0: v.cy + (N / 2 - 0.5) * step, step, gen };
    const rows = Math.ceil(N / (workers.length * 2)), maxit = maxIter(v);
    pending = 0; jobStart = performance.now();
    for (let y0 = 0, i = 0; y0 < N; y0 += rows, i++) {
      workers[i % workers.length].postMessage({ gen, y0, w: N, rows: Math.min(rows, N - y0),
        re0: back.re0, im0: back.im0 - y0 * step, step, maxit });
      pending++;
    }
  }

  /** A band is done; when the picture is complete it becomes the landscape. */
  function onRows(d) {
    if (!back || d.gen !== gen) return;
    back.buf.set(new Uint32Array(d.buf), d.y0 * N);
    if (--pending > 0) return;
    lastMs = performance.now() - jobStart;
    front = back; back = null;
    buildHeights(front.buf);
  }

  /** Histogram-equalised heights: share of outside points with more iterations (1 = peaks). */
  function buildHeights(buf) {
    hist.fill(0);
    let outside = 0;
    for (let i = 0; i < buf.length; i++) {
      const v = buf[i];
      if (v !== INSIDE) { hist[Math.min(v >>> 4, HIST_BINS - 1)]++; outside++; }
    }
    for (let b = 0, sum = 0; b < HIST_BINS; b++) { cdf[b] = sum; sum += hist[b]; }
    for (let i = 0; i < buf.length; i++) {
      const v = buf[i];
      if (v === INSIDE || !outside) { heights[i] = 0; continue; }
      const b = Math.min(v >>> 4, HIST_BINS - 1);
      heights[i] = (1 - (cdf[b] + hist[b] * ((v & 15) / 16)) / outside) * 1000;
    }
    buildLight();
  }

  /** Brightness per voxel from the slope of the height map (light from the upper left). */
  function buildLight() {
    const h = (x, y) => heights[Math.min(N - 1, Math.max(0, y)) * N + Math.min(N - 1, Math.max(0, x))];
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const g = (h(x + 1, y) - h(x - 1, y)) + (h(x, y + 1) - h(x, y - 1));
        light[y * N + x] = Math.max(60, Math.min(255, 170 - g * 0.45));
      }
    }
  }

  /** ImageData of the render size and the sky gradient (BG at the top, FOG at the bottom). */
  function resize(W, H) {
    if (img && img.width === W && img.height === H) return;
    canvas.width = W; canvas.height = H;
    img = ctx.createImageData(W, H);
    px = new Uint32Array(img.data.buffer);
    sky = new Uint32Array(H);
    for (let y = 0; y < H; y++) sky[y] = mix(BG, FOG, Math.floor(y * 256 / H));
  }

  /** Beat pulse from the director's row (a stronger one on the first beat of a bar). */
  function updatePulse(v, now) {
    const dt = Math.min(0.1, (now - (lastT || now)) / 1000); lastT = now;
    if (v.m) {
      const r = Math.floor(v.rowF), key = v.m.order + ':' + r;
      if (key !== rowKey) { rowKey = key; if (r % 4 === 0) pulse = r % 16 === 0 ? 1 : 0.55; }
    }
    pulse *= Math.exp(-dt * 6);
  }

  /** Orbiting camera: once a minute around the centre, looking past it to the left. */
  function camera(now, H) {
    const t = (now - t0) / 1000, ang = t * 0.105, r = 0.85 + 0.15 * Math.sin(t * 0.23), yaw = ang + Math.PI + 0.55;
    const fx = Math.cos(yaw), fy = Math.sin(yaw);
    return { x: r * Math.cos(ang), y: r * Math.sin(ang), z: 0.85 + 0.06 * Math.sin(t * 0.17) - 0.04 * pulse,
      fx, fy, rx: fy, ry: -fx, focal: H * 0.85, horizon: H * 0.24 };
  }

  /** Marches the ray of column `col` front to back and draws the voxels that show. */
  function column(col, W, H, cam, map, shift, flash) {
    const off = (col - W / 2 + 0.5) / cam.focal, dx = cam.fx + cam.rx * off, dy = cam.fy + cam.ry * off;
    const buf = front.buf;
    let ybuf = H;
    for (let z = 0.06; z < ZMAX && ybuf > 0; z += z * 0.011 + 0.0015) {
      const wx = cam.x + dx * z, wy = cam.y + dy * z;
      if (wx < -EXTENT || wx > EXTENT || wy < -EXTENT || wy > EXTENT) continue;
      const bx = map.x0 + wx * map.k, by = map.y0 - wy * map.k;
      if (bx < 0 || by < 0 || bx >= N || by >= N) continue;
      const i = (by | 0) * N + (bx | 0);
      const sy = Math.floor(cam.horizon + (cam.z - heights[i] * (HMAX / 1000)) * cam.focal / z);
      if (sy >= ybuf) continue;
      const v = buf[i];
      let c = dim(v === INSIDE ? BLACK : pal[(v + shift) % 2048], light[i] + 1);
      const fog = z / ZMAX;
      c = mix(c, FOG, Math.floor(fog * fog * 230));
      if (flash > 0) c = mix(c, WHITE, flash);
      for (let y = Math.max(sy, 0); y < ybuf; y++) px[y * W + col] = c;
      ybuf = sy;
    }
  }

  return {
    /** Compiles the kernel and starts the workers; false without WebAssembly SIMD. */
    async init() {
      try {
        const kernel = await WebAssembly.compile(await (await fetch('mandel.wasm')).arrayBuffer());
        const url = URL.createObjectURL(new Blob([WSRC], { type: 'text/javascript' }));
        const n = Math.max(2, Math.min(4, navigator.hardwareConcurrency || 4));
        for (let i = 0; i < n; i++) {
          const w = new Worker(url);
          w.postMessage({ module: kernel });
          w.onmessage = (e) => onRows(e.data);
          workers.push(w);
        }
        return true;
      } catch (e) { console.error('Voxel:', e); return false; }
    },
    /** Switches to the next palette of the Mandelbrot engine. */
    nextPalette() { palIdx = (palIdx + 1) % Mandel.PALETTES.length; pal = Mandel.makePalette(Mandel.PALETTES[palIdx]); },
    /** Resolution and time of the last height map, for the HUD. */
    get info() { return `Voxel · ${N}×${N} · ${Math.round(lastMs)} ms`; },
    /** Draws view `v` into a canvas of cssW x cssH CSS pixels at `dpr` device pixels each. */
    draw(v, cssW, cssH, dpr) {
      const now = performance.now();
      if (!back) start(v);
      updatePulse(v, now);
      const W = Math.min(MAX_W, Math.round(cssW * dpr)), H = Math.max(1, Math.round(cssH * W / cssW));
      resize(W, H);
      for (let y = 0; y < H; y++) px.fill(sky[y], y * W, y * W + W);
      if (front) {
        const cam = camera(now, H);
        const map = { x0: (v.cx - front.re0) / front.step, y0: (front.im0 - v.cy) / front.step, k: v.size * 0.5 / front.step };
        const shift = (((Math.round(v.shift || 0)) % 2048) + 2048) % 2048;
        const flash = Math.floor(Math.max(0, Math.min(0.6, v.flash || 0)) * 256);
        for (let col = 0; col < W; col++) column(col, W, H, cam, map, shift, flash);
      }
      ctx.putImageData(img, 0, 0);
    },
  };
}

self.Voxel = Voxel;
})();
