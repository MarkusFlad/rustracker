// Glue between a page and the Rust framebuffer renderer (src/vis.rs) in modplayer.wasm,
// shared by both editions: the framebuffer edition draws its whole interface with it, the
// classic edition its voxel views (landscape and lake).
//
// The renderer runs in its own WebAssembly instance on the main thread. It needs the song
// (patterns, for the music director), the worklet's scope/spectrum messages and, for the
// Mandelbrot pictures, the SIMD kernel (kernel/mandel_wasm.rs) in Web Workers: Rust hands
// out jobs (vis_mb_take), the workers render rows of palette indices.
(function () {
'use strict';

/** Worker source: renders `rows` rows with the kernel and sends the indices back. */
const WSRC = `let ex, base, ready;
onmessage = async (e) => {
  const d = e.data;
  if (d.module) { ready = WebAssembly.instantiate(d.module, {}).then((i) => { ex = i.exports; base = ex.memory.buffer.byteLength; }); return; }
  await ready;
  const n = d.w * d.rows * 4, need = base + n - ex.memory.buffer.byteLength;
  if (need > 0) ex.memory.grow(Math.ceil(need / 65536));
  ex.render(base, d.w, d.rows, d.re0, d.im0, d.step, d.maxit, 0, 1 << 30, 16, 0);
  const buf = ex.memory.buffer.slice(base, base + n);
  postMessage({ gen: d.gen, y0: d.y0, w: d.w, rows: d.rows, buf }, [buf]);
};`;

/**
 * Creates a renderer instance from the bytes of modplayer.wasm.
 * Returns { vi (the exports), load, push, startWorkers, jobs, show }.
 */
async function RustVis(wasmBytes) {
  const vi = (await WebAssembly.instantiate(wasmBytes, {})).instance.exports;
  let pool = [], next = 0;

  /** Gives the renderer a song (Uint8Array of a MOD file); restarts its views. */
  function load(u8) {
    new Uint8Array(vi.memory.buffer, vi.mp_mod_buffer(), u8.length).set(u8);
    vi.mp_load(u8.length, 44100);
    vi.vis_event(2);
  }

  /** Copies a 'scope' message of the worklet into the next slot; `t` = when it is heard (ms). */
  function push(m, t) {
    const n = Math.min(m.channels, 8), L = m.len, off = vi.vis_spec_offset();
    const slot = new Float32Array(vi.memory.buffer, vi.vis_slot_ptr(), off + 2049);
    slot.set(m.data.subarray(0, n * L), 0);                                   // channel scopes
    slot.set(m.data.subarray(m.channels * L, (m.channels + 2) * L), n * L);   // master L, R
    if (m.spec) slot.set(m.spec, off);
    vi.vis_commit(t, m.order, m.row, m.pattern, m.speed, m.tempo, n, m.sampleRate);
  }

  /** Copies finished rows into the renderer's back buffer unless the job is outdated. */
  function onRows(e) {
    const d = e.data;
    if (d.gen !== vi.vis_mb_gen()) return;
    new Uint32Array(vi.memory.buffer, vi.vis_mb_back_ptr() + d.y0 * d.w * 4, d.rows * d.w).set(new Uint32Array(d.buf));
    vi.vis_mb_rows(d.gen, d.rows);
  }

  /** Starts the worker pool; without SIMD or workers Rust renders on the main thread. */
  async function startWorkers(kernelUrl) {
    const n = Math.max(2, Math.min(8, navigator.hardwareConcurrency || 4));
    try {
      const kernel = await WebAssembly.compile(await (await fetch(kernelUrl)).arrayBuffer());
      const url = URL.createObjectURL(new Blob([WSRC], { type: 'text/javascript' }));
      for (let i = 0; i < n; i++) {
        const w = new Worker(url);
        w.postMessage({ module: kernel });
        w.onmessage = onRows;
        pool.push(w);
      }
      vi.vis_mb_workers(n);
    } catch (e) { console.warn('Mandelbrot workers unavailable, Rust renders on the main thread:', e); pool = []; }
    return pool.length;
  }

  /** Hands a new Mandelbrot job (if any) to the workers, about three row bands per worker. */
  function jobs() {
    if (!pool.length) return;
    const gen = vi.vis_mb_take();
    if (!gen) return;
    const [re0, im0, step, w, h, maxit] = [0, 1, 2, 3, 4, 5].map(vi.vis_mb_param);
    const rows = Math.max(1, Math.ceil(h / (pool.length * 3)));
    for (let y0 = 0; y0 < h; y0 += rows) {
      pool[next++ % pool.length].postMessage({ gen, y0, w, rows: Math.min(rows, h - y0), re0, im0: im0 - y0 * step, step, maxit });
    }
  }

  /**
   * Puts the last frame (pointer `p` from vis_frame or vis_view) onto a canvas and resizes
   * the canvas to it.
   */
  function show(canvas, ctx, p) {
    const fw = vi.vis_width(), fh = vi.vis_height();
    if (canvas.width !== fw || canvas.height !== fh) { canvas.width = fw; canvas.height = fh; }
    ctx.putImageData(new ImageData(new Uint8ClampedArray(vi.memory.buffer, p, fw * fh * 4), fw, fh), 0, 0);
  }

  return { vi, load, push, startWorkers, jobs, show };
}

/**
 * Frame size for a canvas of cssW x cssH CSS pixels: device resolution (dpr at most 2),
 * reduced to about one megapixel, the size of the renderer's framebuffer.
 * Returns { w, h, css } for vis_frame / vis_view.
 */
RustVis.frameSize = (cssW, cssH) => {
  const dpr = Math.min(devicePixelRatio || 1, 2), w = cssW * dpr, h = cssH * dpr;
  const k = Math.min(1, Math.sqrt(1e6 / (w * h)));
  return { w: Math.round(w * k), h: Math.round(h * k), css: dpr * k };
};

self.RustVis = RustVis;
})();
