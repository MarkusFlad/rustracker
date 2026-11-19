// AudioWorklet: runs the Rust/WASM replayer on the audio thread and streams
// scope data (per channel + master L/R) back to the main thread.

const SCOPE_LEN = 1024; // frames per scope message (~23 ms at 44.1 kHz)
const FFT_SIZE = 4096; // FFT length for the spectrum (75 % overlap between messages)

class ModProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ex = null;
    this.loaded = false;
    this.playing = false;
    this.nch = 0;
    this.fill = 0;
    this.scope = null;
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  async onMessage(m) {
    try {
      if (m.type === 'init') {
        // Provide stubs for any unexpected imports so instantiation never fails.
        // The wasm arrives as raw bytes (works in every browser's worklet).
        const module = await WebAssembly.compile(m.wasmBytes);
        const imports = {};
        for (const imp of WebAssembly.Module.imports(module)) {
          (imports[imp.module] ??= {})[imp.name] = () => 0;
        }
        const inst = await WebAssembly.instantiate(module, imports);
        this.ex = inst.exports;
        this.port.postMessage({ type: 'ready' });
      } else if (m.type === 'load') {
        const bytes = new Uint8Array(m.bytes);
        if (bytes.length > this.ex.mp_mod_capacity()) throw new Error('MOD-Datei zu groß');
        this.playing = false;
        new Uint8Array(this.ex.memory.buffer, this.ex.mp_mod_buffer(), bytes.length).set(bytes);
        const n = this.ex.mp_load(bytes.length, sampleRate);
        this.loaded = n > 0;
        this.nch = Math.max(n, 0);
        this.scope = new Float32Array((this.nch + 2) * SCOPE_LEN);
        this.fill = 0;
        this.lastLoops = 0;
        this.port.postMessage({ type: 'loaded', ok: this.loaded, channels: this.nch, error: n });
      } else if (m.type === 'play') {
        this.playing = !!m.on;
      } else if (m.type === 'seek') {
        if (this.loaded) this.ex.mp_set_position(m.order);
      } else if (m.type === 'loop') {
        this.loop = !!m.on;
      } else if (m.type === 'separation') {
        if (this.loaded) this.ex.mp_set_separation(m.value);
      }
    } catch (err) {
      this.port.postMessage({ type: 'error', message: String(err) });
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const L = out[0];
    const R = out[1] || out[0];
    const n = L.length;
    if (!this.loaded || !this.playing) return true;

    const ex = this.ex;
    const mixPtr = ex.mp_render(n);
    const chPtr = ex.mp_channels_ptr();
    const mem = ex.memory.buffer; // read after render: memory may have grown
    const mix = new Float32Array(mem, mixPtr, n * 2);
    const chans = new Float32Array(mem, chPtr, this.nch * n);

    const take = Math.min(n, SCOPE_LEN - this.fill);
    const mo = this.nch * SCOPE_LEN;
    for (let i = 0; i < n; i++) {
      const l = mix[2 * i], r = mix[2 * i + 1];
      L[i] = l;
      R[i] = r;
      if (i < take) {
        this.scope[mo + this.fill + i] = l;
        this.scope[mo + SCOPE_LEN + this.fill + i] = r;
      }
    }
    for (let c = 0; c < this.nch; c++) {
      this.scope.set(chans.subarray(c * n, c * n + take), c * SCOPE_LEN + this.fill);
    }
    this.fill += take;

    if (this.fill >= SCOPE_LEN) {
      this.fill = 0;
      const loops = ex.mp_loops();
      const ended = loops !== this.lastLoops;
      this.lastLoops = loops;
      if (ended && !this.loop) {
        this.playing = false;
        ex.mp_set_position(0);
      }
      const data = this.scope.slice();
      // spectrum: Rust FFT (4096 points, Hann) over the newest samples of the mix
      const sp = ex.mp_spectrum ? ex.mp_spectrum(FFT_SIZE) : 0;
      const spec = sp ? new Float32Array(ex.memory.buffer, sp, FFT_SIZE / 2 + 1).slice() : null;
      this.port.postMessage({ spec, fftSize: FFT_SIZE, sampleRate,
        type: 'scope', data, len: SCOPE_LEN, channels: this.nch,
        order: ex.mp_order(), row: ex.mp_row(), pattern: ex.mp_pattern(),
        speed: ex.mp_speed(), tempo: ex.mp_tempo(), ended: ended && !this.loop,
      }, spec ? [data.buffer, spec.buffer] : [data.buffer]);
    }
    return true;
  }
}

registerProcessor('mod-processor', ModProcessor);
