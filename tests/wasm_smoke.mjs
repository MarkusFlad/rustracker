// Smoke test for the WebAssembly build, run with Node (no browser needed).
//
//   node tests/wasm_smoke.mjs <modplayer.wasm> <song.mod> [native.wav]
//
// Loads the module exactly like the AudioWorklet does, renders 10 seconds and checks:
//  * the C ABI is complete and needs no imports,
//  * the song parses and produces sound (finite, not silent, not clipping),
//  * the Rust FFT returns a sensible spectrum and rejects bad sizes,
//  * optionally: the output matches a native render from `mod2wav` (±2 LSB).
import fs from 'node:fs';

const [wasmPath, modPath, wavPath] = process.argv.slice(2);
if (!wasmPath || !modPath) {
  console.error('usage: node tests/wasm_smoke.mjs <modplayer.wasm> <song.mod> [native.wav]');
  process.exit(2);
}
let failed = 0;
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) failed++; };

const bytes = fs.readFileSync(wasmPath);
const module = await WebAssembly.compile(bytes);
check(WebAssembly.Module.imports(module).length === 0, 'no imports');
const exportsNeeded = ['memory', 'mp_mod_buffer', 'mp_mod_capacity', 'mp_load', 'mp_render', 'mp_channels_ptr',
  'mp_num_channels', 'mp_order', 'mp_row', 'mp_pattern', 'mp_song_length', 'mp_speed', 'mp_tempo', 'mp_loops',
  'mp_set_position', 'mp_set_separation', 'mp_set_gain', 'mp_spectrum'];
const names = WebAssembly.Module.exports(module).map((e) => e.name);
check(exportsNeeded.every((n) => names.includes(n)), `all ${exportsNeeded.length} exports present`);
const ex = (await WebAssembly.instantiate(module, {})).exports;

const song = fs.readFileSync(modPath);
check(song.length <= ex.mp_mod_capacity(), 'song fits into the module buffer');
new Uint8Array(ex.memory.buffer, ex.mp_mod_buffer(), song.length).set(song);
const nch = ex.mp_load(song.length, 44100);
check(nch > 0 && nch === ex.mp_num_channels(), `song parsed (${nch} channels, ${ex.mp_song_length()} positions)`);
check(ex.mp_load(100, 44100) < 0, 'too-short input is rejected');
ex.mp_load(song.length, 44100);

const frames = 1024, blocks = Math.ceil(10 * 44100 / frames);
const pcm = new Int16Array(blocks * frames * 2);
let finite = true, sumsq = 0, peak = 0, chanEnergy = 0;
for (let b = 0; b < blocks; b++) {
  const p = ex.mp_render(frames);
  const mix = new Float32Array(ex.memory.buffer, p, frames * 2);
  for (let i = 0; i < frames * 2; i++) {
    const v = mix[i];
    if (!Number.isFinite(v)) finite = false;
    sumsq += v * v; peak = Math.max(peak, Math.abs(v));
    pcm[b * frames * 2 + i] = Math.round(Math.max(-1, Math.min(1, v)) * 32767);
  }
  const ch = new Float32Array(ex.memory.buffer, ex.mp_channels_ptr(), nch * frames);
  for (let i = 0; i < ch.length; i++) chanEnergy += ch[i] * ch[i];
}
const rms = Math.sqrt(sumsq / pcm.length);
check(finite, 'output is finite');
check(rms > 0.01, `output is not silent (rms ${rms.toFixed(3)})`);
check(peak < 1.0, `output does not clip (peak ${peak.toFixed(3)})`);
check(chanEnergy > 0, 'per-channel scope buffers are filled');
check(ex.mp_order() > 0 || ex.mp_row() > 0, `playback advanced (position ${ex.mp_order()}, row ${ex.mp_row()})`);

const sp = ex.mp_spectrum(4096);
check(sp !== 0, 'mp_spectrum(4096) returns data');
if (sp) {
  const pw = new Float32Array(ex.memory.buffer, sp, 2049);
  let total = 0, kmax = 1;
  for (let k = 1; k < pw.length; k++) { total += pw[k]; if (pw[k] > pw[kmax]) kmax = k; }
  check(pw.every(Number.isFinite) && total > 1e-6, `spectrum has energy (peak at ${(kmax * 44100 / 4096).toFixed(0)} Hz)`);
}
check(ex.mp_spectrum(1000) === 0, 'mp_spectrum rejects non-power-of-two sizes');

if (wavPath) {
  const wav = fs.readFileSync(wavPath).subarray(44);
  const ref = new Int16Array(wav.buffer, wav.byteOffset, Math.floor(wav.length / 2));
  const n = Math.min(ref.length, pcm.length);
  let maxd = 0;
  for (let i = 0; i < n; i++) maxd = Math.max(maxd, Math.abs(ref[i] - pcm[i]));
  check(n > 44100 && maxd <= 2, `matches native render over ${(n / 88200).toFixed(1)} s (max diff ${maxd} LSB)`);
}

console.log(failed ? `\n${failed} check(s) failed` : '\nall checks passed');
process.exit(failed ? 1 : 0);
