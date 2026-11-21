// Smoke test for the framebuffer renderer (www-fb), run with Node.
//
//   node tests/vis_smoke.mjs <modplayer.wasm> <song.mod> [mandel.wasm]
//
// Two instances like in the browser: one plays the song (worklet role), the other draws.
// Feeds 12 s of audio data, draws every view at 60 fps and checks that each view paints,
// that the Rust-drawn buttons react, and how long a frame takes. With mandel.wasm the
// Mandelbrot rows come from the SIMD kernel through the worker job protocol (run inline).
import fs from 'node:fs';

const [wasmPath, modPath, kernelPath] = process.argv.slice(2);
if (!wasmPath || !modPath) { console.error('usage: node tests/vis_smoke.mjs <modplayer.wasm> <song.mod>'); process.exit(2); }
let failed = 0;
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) failed++; };

const bytes = fs.readFileSync(wasmPath);
const mk = async () => (await WebAssembly.instantiate(bytes, {})).instance.exports;
const au = await mk(), vi = await mk();
const song = fs.readFileSync(modPath);
for (const ex of [au, vi]) { new Uint8Array(ex.memory.buffer, ex.mp_mod_buffer(), song.length).set(song); ex.mp_load(song.length, 44100); }
check(typeof vi.vis_frame === 'function' && typeof vi.vis_commit === 'function', 'renderer exports present');
vi.vis_event(2); vi.vis_set_playing(1);
let kernel = null, kbase = 0, jobs = 0;
if (kernelPath) {
  kernel = (await WebAssembly.instantiate(fs.readFileSync(kernelPath), {})).instance.exports;
  kbase = kernel.memory.buffer.byteLength;
  vi.vis_mb_workers(1);
}
// what the Web Workers do, done inline: take a job, render it with the kernel, hand the rows back
const runJobs = () => {
  if (!kernel) return;
  const gen = vi.vis_mb_take();
  if (!gen) return;
  const [re0, im0, step, w, h, maxit] = [0, 1, 2, 3, 4, 5].map(vi.vis_mb_param);
  const n = w * h * 4, need = kbase + n - kernel.memory.buffer.byteLength;
  if (need > 0) kernel.memory.grow(Math.ceil(need / 65536));
  kernel.render(kbase, w, h, re0, im0, step, maxit, 0, 1 << 30, 16, 0);
  new Uint32Array(vi.memory.buffer, vi.vis_mb_back_ptr(), w * h).set(new Uint32Array(kernel.memory.buffer, kbase, w * h));
  vi.vis_mb_rows(gen, h);
  jobs++;
};

const W = 1024, H = 640, nch = au.mp_num_channels(), n = Math.min(nch, 8), off = vi.vis_spec_offset();
const views = [['oscilloscope', null], ['mandelbrot', 'v'], ['spectrum 2D', 'v'], ['spectrum 3D', '3'], ['cube', 'v'], ['voxel', 'v'], ['lake', 'v'], ['lake coarse', 'r']];
let t = 0, frameT = 0;
for (const [name, key] of views) {
  if (key) vi.vis_key(key.charCodeAt(0));
  const times = [];
  let last = 0;
  for (let block = 0; block < 105; block++) {             // ~2.4 s per view
    const mp = au.mp_render(1024), cp = au.mp_channels_ptr(), sp = au.mp_spectrum(4096);
    const mix = new Float32Array(au.memory.buffer, mp, 2048), ch = new Float32Array(au.memory.buffer, cp, nch * 1024);
    const slot = new Float32Array(vi.memory.buffer, vi.vis_slot_ptr(), off + 2049);
    slot.set(ch.subarray(0, n * 1024), 0);
    for (let i = 0; i < 1024; i++) { slot[n * 1024 + i] = mix[2 * i]; slot[(n + 1) * 1024 + i] = mix[2 * i + 1]; }
    slot.set(new Float32Array(au.memory.buffer, sp, 2049), off);
    t += 1024 / 44.1;
    vi.vis_commit(t, au.mp_order(), au.mp_row(), au.mp_pattern(), au.mp_speed(), au.mp_tempo(), n, 44100);
    while (frameT <= t) {
      const t0 = performance.now();
      last = vi.vis_frame(frameT, W, H, 1);
      times.push(performance.now() - t0);
      runJobs();
      frameT += 1000 / 60;
    }
  }
  const px = new Uint32Array(vi.memory.buffer, last, vi.vis_width() * vi.vis_height());
  const colours = new Set();
  for (let i = 0; i < px.length; i += 97) colours.add(px[i]);
  times.sort((a, b) => a - b);
  check(colours.size > 20, `${name}: draws (${colours.size} colours sampled, median ${times[times.length >> 1].toFixed(1)} ms/frame)`);
}
check(vi.vis_width() === W && vi.vis_height() === H, 'frame has the requested size');
if (kernel) check(jobs > 10, `Mandelbrot pictures rendered by the SIMD kernel: ${jobs}`);
check(vi.vis_pointer(10, 10) === 1, 'play button (top left) returns the play action');
check(vi.vis_key(' '.charCodeAt(0)) === 1 && vi.vis_key('s'.charCodeAt(0)) === 2, 'keyboard actions');

console.log(failed ? `\n${failed} check(s) failed` : '\nall checks passed');
process.exit(failed ? 1 : 0);
