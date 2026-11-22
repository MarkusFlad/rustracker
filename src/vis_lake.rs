//! Lake view: a lake of voxels that the music sets into motion.
//!
//! The water surface is a 2D wave equation on a grid (discrete Laplacian, slight damping),
//! so wave crests and troughs add up and cancel like real waves and reflect at the shore.
//! Every onset of the deepest bass drops a "stone" into the lake; beats without a bass hit
//! drop a smaller one. The point where they fall circles slowly around the centre of the
//! lake. The lake has an irregular, natural shoreline (a circle deformed by a few
//! harmonics) with a small island. The surface and the hilly shore are drawn with
//! [`crate::voxel_space`] from a camera that slowly circles the lake, drifting away and
//! back, rising and sinking.

use crate::gfx::*;
use crate::math::*;
use crate::vis::*;
use crate::vis_cube::draw_vertical_gradient;
use crate::voxel_space::{render, HeightMap, Orbit, Shore, FOG, HSCALE};

/// Grid cells per side.
pub(crate) const LAKE_N: usize = 240;
/// The grid covers ±LAKE_EXT world units.
const LAKE_EXT: f64 = 2.0;
/// Width of the beach in world units.
const BEACH: f64 = 0.05;
/// Colour of the beach.
const SAND: u32 = rgb(150, 135, 90);
/// Mean radius of the lake in world units.
const LAKE_R: f64 = 0.95;
/// Shoreline: harmonic numbers, relative amplitudes and phases deforming the circle.
const SHORE_N: [u32; 4] = [2, 3, 5, 9];
const SHORE_A: [f64; 4] = [0.15, 0.10, 0.05, 0.025];
const SHORE_P: [f64; 4] = [1.3, 0.4, 2.1, 0.7];
/// The island: centre, mean radius and its own harmonics.
const ISLE_X: f64 = 0.12;
const ISLE_Y: f64 = -0.1;
const ISLE_R: f64 = 0.14;
const ISLE_N: [u32; 4] = [2, 3, 4, 7];
const ISLE_A: [f64; 4] = [0.18, 0.12, 0.08, 0.04];
const ISLE_P: [f64; 4] = [0.5, 2.4, 1.1, 3.0];
/// Wave speed² per step (stable below 0.5).
const C2: f32 = 0.3;
/// Damping per step.
const DAMP: f32 = 0.994;
/// Simulation step in seconds (fixed, independent of the frame rate).
const STEP: f64 = 1.0 / 120.0;
/// Stored height of the water at rest.
const WATER: f64 = 300.0;
/// Stored height per unit of wave amplitude.
const WAVE_K: f64 = 80.0;
/// Bands averaged for the bass level (the lowest ~10 of 96, about 30..75 Hz).
const BASS_BANDS: usize = 10;
/// A slow camera flight: once around the lake in about a minute, drifting away and back
/// over about a minute and a half, rising and sinking over about a minute; it always aims
/// at the lake.
const ORBIT: Orbit = Orbit {
    speed: 0.105,
    radius: 1.35,
    breathe: 0.35,
    breathe_rate: 0.068,
    height: 0.95,
    rise: 0.32,
    rise_rate: 0.104,
    rise_phase: 1.0,
    look: 0.15,
    horizon: 0.0,
    aim: 0.58,
    aim_height: 0.165,
};

impl Vis {
    /// The lake view: listens for bass hits and beats, advances the waves in fixed steps
    /// and renders surface and shore.
    pub(crate) fn draw_lake(&mut self, s: &Surface, now: f64, dt: f64, cur: i32) {
        if !self.lk_ready {
            self.build_shore();
        }
        if cur >= 0 && self.playing {
            self.listen(cur as usize, now);
        }
        self.lk_acc += dt;
        let mut steps = 0;
        while self.lk_acc >= STEP && steps < 8 {
            self.wave_step();
            self.lk_acc -= STEP;
            steps += 1;
        }
        if steps == 8 {
            self.lk_acc = 0.0;
        }
        self.build_water();
        draw_vertical_gradient(s, BG, FOG);
        let cam = ORBIT.camera((now - self.t0) / 1000.0, 0.0, s.h);
        let map = HeightMap {
            heights: (&raw const self.lk_h) as *const u16,
            colours: (&raw const self.lk_c) as *const u32,
            w: LAKE_N as i32,
            h: LAKE_N as i32,
            x0: LAKE_N as f64 / 2.0,
            y0: LAKE_N as f64 / 2.0,
            k: LAKE_N as f64 / (2.0 * LAKE_EXT),
            extent: LAKE_EXT,
            shore: Shore {
                edge: (&raw const self.lk_edge) as *const f32,
                water_heights: (&raw const self.lk_wh) as *const u16,
                water_colours: (&raw const self.lk_wc) as *const u32,
                land_heights: (&raw const self.lk_lh) as *const u16,
                land_colours: (&raw const self.lk_lc) as *const u32,
                beach_colours: (&raw const self.lk_sc) as *const u32,
                beach: BEACH,
                blend: 0.4 / (LAKE_N as f64 / (2.0 * LAKE_EXT)),
            },
        };
        unsafe { render(s, &cam, &map, 0, self.vx_fine) };
        self.draw_lake_hud(s);
    }

    /// Bass onsets (the bass level jumps above its running average) and beats without a
    /// bass hit drop stones into the lake.
    fn listen(&mut self, k: usize, now: f64) {
        let m = self.msgs[k].copy();
        if m.t != self.lk_slot_t {
            self.lk_slot_t = m.t;
            let bass = self.bass_level(k);
            let rise = bass - self.lk_bass_avg;
            self.lk_bass_avg += rise * 0.1;
            self.lk_bass = bass;
            if rise > 0.045 && now - self.lk_hit_t > 150.0 {
                self.drop_stone(now, clampf(rise * 10.0, 0.35, 1.0));
            }
        }
        let key = m.order * 64 + m.row;
        if key != self.lk_rowkey {
            self.lk_rowkey = key;
            if m.row % 4 == 0 && now - self.lk_hit_t > 220.0 {
                self.drop_stone(now, 0.3 + 0.4 * self.inten);
            }
        }
    }

    /// Mean of the lowest spectrum bands of slot `k` (0..1).
    fn bass_level(&self, k: usize) -> f64 {
        let mut sum = 0.0;
        let mut b = 0;
        while b < BASS_BANDS {
            sum += self.bands[k * NB + b] as f64;
            b += 1;
        }
        sum / BASS_BANDS as f64
    }

    /// A stone of the given strength falls at the point circling the centre: a Gaussian
    /// dent in the surface, from which a ring wave spreads.
    fn drop_stone(&mut self, now: f64, strength: f64) {
        self.lk_hit_t = now;
        self.lk_drops += 1;
        let t = (now - self.t0) / 1000.0;
        let k = LAKE_N as f64 / (2.0 * LAKE_EXT);
        let cx = LAKE_N as f64 / 2.0 + cos(t * 0.18) * LAKE_R * 0.45 * k;
        let cy = LAKE_N as f64 / 2.0 + sin(t * 0.18) * LAKE_R * 0.45 * k;
        let amp = 1.0 * strength;
        let cur = self.wave_cur();
        let (x0, y0) = (cx as i32, cy as i32);
        let mut y = y0 - 7;
        while y <= y0 + 7 {
            let mut x = x0 - 7;
            while x <= x0 + 7 {
                let i = (y * LAKE_N as i32 + x) as usize;
                if self.lk_mask[i] != 0 {
                    let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                    let g = exp_neg((dx * dx + dy * dy) / (2.0 * 2.5 * 2.5));
                    unsafe { *at(cur, i) -= (amp * g) as f32 };
                }
                x += 1;
            }
            y += 1;
        }
    }

    /// The current wave buffer (the other one holds the previous step).
    fn wave_cur(&mut self) -> *mut f32 {
        if self.lk_flip {
            (&raw mut self.lk_b) as *mut f32
        } else {
            (&raw mut self.lk_a) as *mut f32
        }
    }

    /// One step of the damped wave equation: next = (2·cur − prev + c²·∇²cur)·damping,
    /// written over `prev`; cells outside the lake stay 0 (the shore reflects).
    fn wave_step(&mut self) {
        let pa = (&raw mut self.lk_a) as *mut f32;
        let pb = (&raw mut self.lk_b) as *mut f32;
        let (c, p) = if self.lk_flip { (pb, pa) } else { (pa, pb) };
        let n = LAKE_N;
        let mut y = 1;
        while y < n - 1 {
            let mut x = 1;
            while x < n - 1 {
                let i = y * n + x;
                if self.lk_mask[i] != 0 {
                    unsafe {
                        let u = *at(c, i);
                        let lap =
                            *at(c, i - 1) + *at(c, i + 1) + *at(c, i - n) + *at(c, i + n) - 4.0 * u;
                        *at(p, i) = (2.0 * u - *at(p, i) + C2 * lap) * DAMP;
                    }
                }
                x += 1;
            }
            y += 1;
        }
        self.lk_flip = !self.lk_flip;
    }

    /// Height and lit colour of every water cell from the current surface: teal water,
    /// lighter on crests, white foam on high crests, lit by the slope.
    fn build_water(&mut self) {
        let c = self.wave_cur();
        let n = LAKE_N;
        let mut i = 0;
        while i < n * n {
            if self.lk_mask[i] != 0 {
                let u = unsafe { *at(c, i) } as f64;
                let gx = unsafe { *at(c, i + 1) - *at(c, i - 1) } as f64;
                let gy = unsafe { *at(c, i + n) - *at(c, i - n) } as f64;
                let (h, c) = (
                    clampf(WATER + u * WAVE_K, 0.0, HSCALE) as u16,
                    water_colour(u, gx + gy),
                );
                self.lk_h[i] = h;
                self.lk_c[i] = c;
                self.lk_wh[i] = h;
                self.lk_wc[i] = c;
            }
            i += 1;
        }
    }

    /// The lake mask and the static shore: hills rising away from the water, sand at the
    /// waterline, grass, then rock higher up, lit by the slope.
    fn build_shore(&mut self) {
        let n = LAKE_N as i32;
        let mut y = 0;
        while y < n {
            let mut x = 0;
            while x < n {
                // land() and shore_height() are expensive (many seconds on a Pentium for
                // the whole grid): computed once per cell and kept for the layers below
                let (wx, wy) = cell_world(x, y);
                let i = (y * n + x) as usize;
                let l = land(wx, wy);
                let sh = shore_height(wx, wy) as u16;
                let inside = l < 0.0 && x > 0 && y > 0 && x < n - 1 && y < n - 1;
                self.lk_mask[i] = if inside { 1 } else { 0 };
                self.lk_beach[i] = if l < BEACH { 1 } else { 0 };
                self.lk_edge[i] = l as f32;
                self.lk_lh[i] = sh;
                self.lk_h[i] = if inside { WATER as u16 } else { sh };
                x += 1;
            }
            y += 1;
        }
        y = 0;
        while y < n {
            let mut x = 0;
            while x < n {
                let i = (y * n + x) as usize;
                if self.lk_mask[i] == 0 {
                    self.lk_c[i] = self.shore_colour(x, y);
                }
                x += 1;
            }
            y += 1;
        }
        self.build_shore_layers();
        self.lk_ready = true;
    }

    /// The layers for the smooth shoreline of the fine mode (distance to the shoreline and
    /// land below the water are already filled in by `build_shore`): the land and beach
    /// colours lit by the land's slope, and calm water on land cells.
    fn build_shore_layers(&mut self) {
        let n = LAKE_N as i32;
        let mut y = 0;
        while y < n {
            let mut x = 0;
            while x < n {
                let i = (y * n + x) as usize;
                if self.lk_mask[i] == 0 {
                    self.lk_wh[i] = WATER as u16;
                    self.lk_wc[i] = water_colour(0.0, 0.0);
                }
                x += 1;
            }
            y += 1;
        }
        y = 0;
        while y < n {
            let mut x = 0;
            while x < n {
                let i = (y * n + x) as usize;
                let g = (self.land_height(x + 1, y) - self.land_height(x - 1, y))
                    + (self.land_height(x, y + 1) - self.land_height(x, y - 1));
                let light = clampf(175.0 - g * 1.2, 60.0, 255.0) as u32;
                let hh = self.lk_lh[i] as f64;
                self.lk_lc[i] = dim(grass_colour(hh), light);
                self.lk_sc[i] = dim(SAND, light);
                x += 1;
            }
            y += 1;
        }
    }

    /// Height of the land layer at cell (x, y), clamped to the grid.
    fn land_height(&self, x: i32, y: i32) -> f64 {
        let n = LAKE_N as i32;
        self.lk_lh[(iclamp(y, 0, n - 1) * n + iclamp(x, 0, n - 1)) as usize] as f64
    }

    /// Lit colour of shore cell (x, y).
    fn shore_colour(&self, x: i32, y: i32) -> u32 {
        let n = LAKE_N as i32;
        let g = (self.lake_height(x + 1, y) - self.lake_height(x - 1, y))
            + (self.lake_height(x, y + 1) - self.lake_height(x, y - 1));
        let i = (y * n + x) as usize;
        let hh = self.lk_h[i] as f64;
        let base = if self.lk_beach[i] != 0 {
            SAND
        } else {
            grass_colour(hh)
        };
        dim(base, clampf(175.0 - g * 1.2, 60.0, 255.0) as u32)
    }

    /// Stored height of cell (x, y), clamped to the grid.
    fn lake_height(&self, x: i32, y: i32) -> f64 {
        let n = LAKE_N as i32;
        self.lk_h[(iclamp(y, 0, n - 1) * n + iclamp(x, 0, n - 1)) as usize] as f64
    }

    /// Bass level and number of stones, top right.
    fn draw_lake_hud(&self, s: &Surface) {
        let fs = self.fs;
        let mut t = TextBuf::new();
        t.s(b"SEE  BASS ");
        t.num((self.lk_bass * 100.0) as u32, 1, 48);
        t.s(b"%  WELLEN ");
        t.num(self.lk_drops, 1, 48);
        let tw = t.n as i32 * 6 * fs;
        s.fill_a(s.w - tw - 6 * fs, 2 * fs, tw + 4 * fs, 10 * fs, BLACK, 150);
        s.text_buf(s.w - tw - 4 * fs, 4 * fs, &t, fs, WHITE);
    }
}

/// Element `i` of a float buffer.
#[inline(always)]
fn at(p: *mut f32, i: usize) -> *mut f32 {
    (p as usize + i * 4) as *mut f32
}

/// World coordinates of the centre of grid cell (x, y).
fn cell_world(x: i32, y: i32) -> (f64, f64) {
    let k = LAKE_N as f64 / (2.0 * LAKE_EXT);
    (
        (x as f64 + 0.5 - LAKE_N as f64 / 2.0) / k,
        (LAKE_N as f64 / 2.0 - y as f64 - 0.5) / k,
    )
}

/// Radius of a deformed circle in direction (c, s) = (cos θ, sin θ): mean radius `r`
/// times 1 + Σ a·sin(nθ + p), with `trig` = [cos p₀..p₃, sin p₀..p₃] (see [`phase_trig`]).
fn blob_radius(c: f64, s: f64, r: f64, ns: &[u32; 4], amps: &[f64; 4], trig: &[f64; 8]) -> f64 {
    let mut f = 1.0;
    let (mut cn, mut sn) = (1.0, 0.0); // cos nθ, sin nθ
    let mut n = 1;
    let mut j = 0;
    while j < 4 {
        while n <= ns[j] {
            let t = cn * c - sn * s;
            sn = sn * c + cn * s;
            cn = t;
            n += 1;
        }
        f += amps[j] * (sn * trig[j] + cn * trig[4 + j]);
        j += 1;
    }
    r * f
}

/// Cosines and sines of the shoreline and island phases, computed once: [`blob_radius`]
/// runs for every cell of the lake, and recomputing them there made building the lake take
/// many seconds on a Pentium.
static mut SHORE_TRIG: [f64; 8] = [0.0; 8];
static mut ISLE_TRIG: [f64; 8] = [0.0; 8];
static mut TRIG_READY: bool = false;

/// [cos p₀..p₃, sin p₀..p₃] of four phases.
fn phase_trig(p: &[f64; 4]) -> [f64; 8] {
    [
        cos(p[0]),
        cos(p[1]),
        cos(p[2]),
        cos(p[3]),
        sin(p[0]),
        sin(p[1]),
        sin(p[2]),
        sin(p[3]),
    ]
}

/// The phase tables of shoreline and island (filled on first use).
fn lake_trig() -> (&'static [f64; 8], &'static [f64; 8]) {
    unsafe {
        if !TRIG_READY {
            SHORE_TRIG = phase_trig(&SHORE_P);
            ISLE_TRIG = phase_trig(&ISLE_P);
            TRIG_READY = true;
        }
        (&*(&raw const SHORE_TRIG), &*(&raw const ISLE_TRIG))
    }
}

/// How far a world point lies on land: the distance beyond the shoreline (positive on the
/// shore and on the island, negative in the water).
fn land(wx: f64, wy: f64) -> f64 {
    let d = sqrt(wx * wx + wy * wy) + 1e-9;
    let shore = d - blob_radius(wx / d, wy / d, LAKE_R, &SHORE_N, &SHORE_A, lake_trig().0);
    maxf(shore, island(wx, wy))
}

/// How far a world point lies inside the island (negative outside).
fn island(wx: f64, wy: f64) -> f64 {
    let (dx, dy) = (wx - ISLE_X, wy - ISLE_Y);
    let d = sqrt(dx * dx + dy * dy) + 1e-9;
    blob_radius(dx / d, dy / d, ISLE_R, &ISLE_N, &ISLE_A, lake_trig().1) - d
}

/// Land height at a world point: hills rising away from the shore, the island as a steeper
/// small hill.
fn shore_height(wx: f64, wy: f64) -> f64 {
    let isle = island(wx, wy);
    if isle > 0.0 {
        return clampf(330.0 + isle * 1500.0, 0.0, HSCALE);
    }
    let l = land(wx, wy);
    let hills = 60.0 * sin(wx * 4.1 + 0.7) * sin(wy * 3.3 + 1.9) + 35.0 * sin(wx * 9.7 - wy * 7.3);
    clampf(
        330.0 + l * 420.0 + hills * clampf(l * 4.0, 0.0, 1.0),
        0.0,
        HSCALE,
    )
}

/// Land colour above the beach: grass, turning to rock higher up.
fn grass_colour(h: f64) -> u32 {
    mix(
        rgb(30, 90, 45),
        rgb(95, 95, 85),
        (clampf((h - 420.0) / 260.0, 0.0, 1.0) * 256.0) as u32,
    )
}

/// Water colour for wave amplitude `u` and slope `g`.
fn water_colour(u: f64, g: f64) -> u32 {
    let deep = rgb(6, 38, 62);
    let crest = rgb(40, 160, 175);
    let mut c = mix(
        deep,
        crest,
        (clampf(0.45 + u * 0.7, 0.0, 1.0) * 256.0) as u32,
    );
    if u > 0.45 {
        c = mix(
            c,
            WHITE,
            (clampf((u - 0.45) * 1.6, 0.0, 0.8) * 256.0) as u32,
        );
    }
    dim(c, clampf(185.0 - g * 600.0, 60.0, 256.0) as u32)
}
