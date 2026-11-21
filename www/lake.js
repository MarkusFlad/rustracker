// Lake view of the classic edition: a lake of voxels that the music sets into motion.
//
// The water surface is a 2D wave equation on a grid (discrete Laplacian, slight damping),
// so crests and troughs add up and cancel like real waves and reflect at the shore. Every
// onset of the deepest bass drops a "stone" into the lake; beats without a bass hit drop a
// smaller one. The point where they fall circles slowly around the centre. The shoreline
// is a circle deformed by a few harmonics, with a small island; the camera circles slowly,
// drifting away and back, rising and sinking. Rendered with the voxel-space renderer from
// voxel.js. Same model as the framebuffer edition
// (src/vis_lake.rs).
(function () {
'use strict';

const { rgb, mix, dim, orbitCamera, renderHeightmap, Target, WHITE } = self.VoxelSpace;
const N = 240;            // grid cells per side
const EXT = 2.0;          // the grid covers ±EXT world units
const R = 0.95;           // mean lake radius (world units)
/** Shoreline and island: deformed circles, Σ amplitude·sin(n·θ + phase) on the radius. */
const SHORE = [[2, 0.15, 1.3], [3, 0.10, 0.4], [5, 0.05, 2.1], [9, 0.025, 0.7]];
const ISLE = { x: 0.12, y: -0.1, r: 0.14, waves: [[2, 0.18, 0.5], [3, 0.12, 2.4], [4, 0.08, 1.1], [7, 0.04, 3.0]] };
const C2 = 0.3;           // wave speed² per step (stable below 0.5)
const DAMP = 0.994;       // damping per step
const STEP = 1000 / 120;  // simulation step in ms
const WATER = 300;        // stored height of the water at rest
const WAVE_K = 80;        // stored height per unit of amplitude
const K = N / (2 * EXT);  // cells per world unit
const BEACH = 0.05;       // beach width (world units)
const SAND = rgb(150, 135, 90);
/** Slow flight: once around in ~1 min, away and back over ~1.5 min, rising and sinking over ~1 min. */
const ORBIT = { speed: 0.105, radius: 1.35, breathe: 0.35, breatheRate: 0.068, height: 0.95, rise: 0.32, riseRate: 0.104,
  risePhase: 1.0, look: 0.15, horizon: 0, aim: 0.58, aimHeight: 0.165 };

/** World coordinates of the centre of cell (x, y). */
const cellWorld = (x, y) => [(x + 0.5 - N / 2) / K, (N / 2 - y - 0.5) / K];

/** Radius of a deformed circle in direction θ. */
const blobRadius = (theta, r, waves) => r * waves.reduce((f, [n, a, p]) => f + a * Math.sin(n * theta + p), 1);
/** How far a point lies inside the island (negative outside). */
function island(wx, wy) {
  const dx = wx - ISLE.x, dy = wy - ISLE.y;
  return blobRadius(Math.atan2(dy, dx), ISLE.r, ISLE.waves) - Math.hypot(dx, dy);
}
/** How far a point lies on land: beyond the shoreline or inside the island (negative in the water). */
function land(wx, wy) {
  return Math.max(Math.hypot(wx, wy) - blobRadius(Math.atan2(wy, wx), R, SHORE), island(wx, wy));
}
/** Land height: hills rising away from the shore, the island as a steeper small hill. */
function shoreHeight(wx, wy) {
  const isle = island(wx, wy);
  if (isle > 0) return Math.min(1000, 330 + isle * 1500);
  const l = land(wx, wy);
  const hills = 60 * Math.sin(wx * 4.1 + 0.7) * Math.sin(wy * 3.3 + 1.9) + 35 * Math.sin(wx * 9.7 - wy * 7.3);
  return Math.max(0, Math.min(1000, 330 + l * 420 + hills * Math.max(0, Math.min(1, l * 4))));
}

/** Land colour above the beach: grass, turning to rock higher up. */
const grassColour = (h) => mix(rgb(30, 90, 45), rgb(95, 95, 85), Math.floor(Math.max(0, Math.min(1, (h - 420) / 260)) * 256));

/** Water colour for amplitude u and slope g: teal, lighter on crests, foam on high crests. */
function waterColour(u, g) {
  let c = mix(rgb(6, 38, 62), rgb(40, 160, 175), Math.floor(Math.max(0, Math.min(1, 0.45 + u * 0.7)) * 256));
  if (u > 0.45) c = mix(c, WHITE, Math.floor(Math.min(0.8, (u - 0.45) * 1.6) * 256));
  return dim(c, Math.floor(Math.max(60, Math.min(256, 185 - g * 600))));
}

/** Strongest power between f0 and f1 Hz of a power spectrum, mapped to 0..1 over -96..0 dB. */
function bandLevel(p, binHz, f0, f1) {
  let v = 0;
  for (let k = Math.floor(f0 / binHz); k <= Math.ceil(f1 / binHz) && k < p.length; k++) if (p[k] > v) v = p[k];
  return Math.max(0, Math.min(1, (10 * Math.log10(v + 1e-14) + 96) / 96));
}

/** Lake(canvas): call draw() every frame with the message being heard (or null). */
function Lake(canvas) {
  const target = Target(canvas);
  let cur = new Float32Array(N * N), prev = new Float32Array(N * N);
  const mask = new Uint8Array(N * N), heights = new Uint16Array(N * N), colours = new Uint32Array(N * N);
  // fine mode: layers for a smooth shoreline (see renderHeightmap in voxel.js)
  const shore = { edge: new Float32Array(N * N), waterHeights: new Uint16Array(N * N), waterColours: new Uint32Array(N * N),
    landHeights: new Uint16Array(N * N), landColours: new Uint32Array(N * N), beachColours: new Uint32Array(N * N),
    beach: BEACH, blend: 0.4 / K };
  const t0 = performance.now();
  let acc = 0, lastT = 0, bassAvg = 0, bass = 0, hitT = -1e9, seq = -1, rowKey = '', drops = 0;

  /** Lake mask and the static shore (sand, grass, rock, lit by the slope). */
  function buildShore() {
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const [wx, wy] = cellWorld(x, y), i = y * N + x;
        const inside = land(wx, wy) < 0 && x > 0 && y > 0 && x < N - 1 && y < N - 1;
        mask[i] = inside ? 1 : 0;
        heights[i] = inside ? WATER : shoreHeight(wx, wy);
      }
    }
    const h = (x, y) => heights[Math.min(N - 1, Math.max(0, y)) * N + Math.min(N - 1, Math.max(0, x))];
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const i = y * N + x;
        if (mask[i]) continue;
        const [wx, wy] = cellWorld(x, y), g = (h(x + 1, y) - h(x - 1, y)) + (h(x, y + 1) - h(x, y - 1));
        const base = land(wx, wy) < BEACH ? SAND : grassColour(heights[i]);
        colours[i] = dim(base, Math.floor(Math.max(60, Math.min(255, 175 - g * 1.2))));
      }
    }
    buildShoreLayers();
  }

  /** Smooth-shoreline layers: distance to the shoreline, land continued below the water, land
   * and beach colours lit by the land's slope, calm water on land cells. */
  function buildShoreLayers() {
    const calm = waterColour(0, 0);
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const [wx, wy] = cellWorld(x, y), i = y * N + x;
        shore.edge[i] = land(wx, wy);
        shore.landHeights[i] = shoreHeight(wx, wy);
        if (!mask[i]) { shore.waterHeights[i] = WATER; shore.waterColours[i] = calm; }
      }
    }
    const lh = shore.landHeights, h = (x, y) => lh[Math.min(N - 1, Math.max(0, y)) * N + Math.min(N - 1, Math.max(0, x))];
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const i = y * N + x, g = (h(x + 1, y) - h(x - 1, y)) + (h(x, y + 1) - h(x, y - 1));
        const light = Math.floor(Math.max(60, Math.min(255, 175 - g * 1.2)));
        shore.landColours[i] = dim(grassColour(lh[i]), light);
        shore.beachColours[i] = dim(SAND, light);
      }
    }
  }
  buildShore();

  /** A stone of the given strength falls at the point circling the centre: a Gaussian dent. */
  function dropStone(now, strength) {
    hitT = now; drops++;
    const t = (now - t0) / 1000;
    const cx = N / 2 + Math.cos(t * 0.18) * R * 0.45 * K, cy = N / 2 + Math.sin(t * 0.18) * R * 0.45 * K;
    for (let y = Math.floor(cy) - 7; y <= Math.floor(cy) + 7; y++) {
      for (let x = Math.floor(cx) - 7; x <= Math.floor(cx) + 7; x++) {
        const i = y * N + x;
        if (mask[i]) cur[i] -= strength * Math.exp(-((x - cx) ** 2 + (y - cy) ** 2) / (2 * 2.5 * 2.5));
      }
    }
  }

  /** Bass onsets (level jumps above its running average) and beats without a bass hit drop stones. */
  function listen(m, now, inten) {
    if (m.seq !== seq) {
      seq = m.seq;
      bass = bandLevel(m.spec, m.sampleRate / m.fftSize, 30, 75);
      const rise = bass - bassAvg;
      bassAvg += rise * 0.1;
      if (rise > 0.045 && now - hitT > 150) dropStone(now, Math.max(0.35, Math.min(1, rise * 10)));
    }
    const key = m.order + ':' + m.row;
    if (key !== rowKey) {
      rowKey = key;
      if (m.row % 4 === 0 && now - hitT > 220) dropStone(now, 0.3 + 0.4 * inten);
    }
  }

  /** One step of the damped wave equation, written over `prev`; then the buffers swap. */
  function step() {
    for (let y = 1; y < N - 1; y++) {
      for (let x = 1, i = y * N + 1; x < N - 1; x++, i++) {
        if (!mask[i]) continue;
        const u = cur[i];
        prev[i] = (2 * u - prev[i] + C2 * (cur[i - 1] + cur[i + 1] + cur[i - N] + cur[i + N] - 4 * u)) * DAMP;
      }
    }
    const t = cur; cur = prev; prev = t;
  }

  /** Heights and lit colours of the water cells from the current surface. */
  function buildWater() {
    for (let i = 0; i < N * N; i++) {
      if (!mask[i]) continue;
      const u = cur[i];
      heights[i] = shore.waterHeights[i] = Math.max(0, Math.min(1000, WATER + u * WAVE_K));
      colours[i] = shore.waterColours[i] = waterColour(u, (cur[i + 1] - cur[i - 1]) + (cur[i + N] - cur[i - N]));
    }
  }

  return {
    /** Bass level and number of stones, for the HUD. */
    get info() { return `See · Bass ${Math.round(bass * 100)} % · ${drops} Wellen`; },
    /**
     * Advances the waves to `now` and draws the lake.
     * m: the worklet message being heard (with spectrum), or null; inten: music level 0..1;
     * fine: smooth surface instead of voxels.
     */
    draw(m, inten, cssW, cssH, dpr, fine) {
      const now = performance.now();
      if (m) listen(m, now, inten);
      acc += Math.min(100, now - (lastT || now)); lastT = now;
      let n = 0;
      for (; acc >= STEP && n < 8; n++) { step(); acc -= STEP; }
      if (n === 8) acc = 0;
      buildWater();
      const { px, W, H } = target.begin(cssW, cssH, dpr, fine);
      renderHeightmap(px, W, H, orbitCamera(ORBIT, (now - t0) / 1000, 0, H),
        { heights, colours, w: N, h: N, x0: N / 2, y0: N / 2, k: K, extent: EXT, shore }, 0, fine);
      target.end();
    },
  };
}

self.Lake = Lake;
})();
