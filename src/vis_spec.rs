//! Spectrum views: 2D bars with peak markers and a spectrogram, and the 3D history.

use crate::gfx::*;
use crate::math::*;
use crate::spectrum::{midi_note, LogScale};
use crate::vis::*;

/// Frequencies labelled below the spectrum axes.
const LABEL_FREQS: [u32; 9] = [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
/// Perspective strength of the 3D spectrum (scale 1/(1 + z·k), z = 0 front .. 1 back).
const PERSP: f64 = 2.4;

/// Where the parts of the 2D spectrum view go.
struct Spec2Layout {
    fs: i32,
    left: i32,
    top: i32,
    gap: i32,
    plot_w: i32,
    bars_h: i32,
}

impl Spec2Layout {
    fn new(s: &Surface, fs: i32, labels: bool) -> Spec2Layout {
        let fs_l = if labels { fs } else { 1 };
        let left = if labels { 22 * fs_l } else { 2 };
        let top = if labels { 14 * fs_l } else { 16 };
        let gap = if labels { 10 * fs_l } else { 2 };
        let bars_h = if labels {
            (s.h - top) * 58 / 100 - gap
        } else {
            s.h - top - 4
        };
        Spec2Layout {
            fs: fs_l,
            left,
            top,
            gap,
            plot_w: s.w - left - 3 * fs_l,
            bars_h,
        }
    }
}

impl Vis {
    /// Highest displayed frequency for the current slot (44.1 kHz if there is none).
    fn fmax_of(&self, cur: i32) -> f64 {
        let sr = if cur >= 0 {
            self.msgs[cur as usize].sr
        } else {
            44100.0
        };
        minf(20000.0, sr / 2.0)
    }

    /// Frequency labels (50 .. 20K) below a log axis of width `w` starting at `x0`.
    pub(crate) fn freq_labels(&self, s: &Surface, x0: i32, y: i32, w: i32, fmax: f64, scale: i32) {
        let axis = LogScale::new(F_MIN, fmax);
        let mut i = 0;
        while i < 9 {
            let fr = LABEL_FREQS[i];
            if fr as f64 <= fmax {
                let x = x0 + axis.x(fr as f64, w as f64) as i32;
                let mut t = TextBuf::new();
                if fr >= 1000 {
                    t.num(fr / 1000, 1, 48);
                    t.ch(75);
                } else {
                    t.num(fr, 1, 48);
                }
                s.text_buf(x - t.n as i32 * 3 * scale, y, &t, scale, PHOS_DIM);
            }
            i += 1;
        }
    }

    /// Lets the displayed bars follow the bands of slot `cur` (instant attack, release
    /// 0.9/s) and the peak markers fall slowly (0.25/s). Frozen while paused.
    pub(crate) fn update_disp(&mut self, dt: f64, cur: i32) {
        if cur < 0 || !self.playing {
            return;
        }
        let k = cur as usize;
        let mut b = 0;
        while b < NB {
            let v = self.bands[k * NB + b];
            let d = self.disp[b] - (dt * 0.9) as f32;
            self.disp[b] = if v > d { v } else { d };
            let p = self.peak[b] - (dt * 0.25) as f32;
            self.peak[b] = if self.disp[b] > p { self.disp[b] } else { p };
            b += 1;
        }
    }

    /// "SPITZE 440.0 HZ = A-4  -12.3 DB" for the strongest peak between 40 Hz and 5 kHz;
    /// leaves `t` empty if there is no slot or no peak.
    pub(crate) fn peak_text(&self, cur: i32, t: &mut TextBuf) {
        if cur < 0 {
            return;
        }
        let pk = self.spectrum(cur as usize).peak(40.0, 5000.0);
        if !pk.found {
            return;
        }
        t.s(b"SPITZE ");
        t.dec1(pk.freq);
        t.s(b" HZ = ");
        t.midi(midi_note(pk.freq));
        t.s(b"  ");
        t.dec1(pk.db);
        t.s(b" DB");
    }

    /// The 2D spectrum: log-frequency bars with falling peak markers and, with
    /// `labels`, a dB scale, frequency labels, the spectrogram and the peak text.
    /// Without labels (cube face) only the bars are drawn.
    pub(crate) fn draw_spec2(&mut self, s: &Surface, dt: f64, cur: i32, labels: bool) {
        self.update_disp(dt, cur);
        let lay = Spec2Layout::new(s, self.fs, labels);
        let fs = lay.fs;
        if labels {
            draw_db_grid(s, &lay);
        }
        self.draw_bars(s, &lay);
        s.rect(lay.left, lay.top, lay.plot_w, lay.bars_h, PHOSPHOR, 60);
        if !labels {
            return;
        }
        self.freq_labels(
            s,
            lay.left,
            lay.top + lay.bars_h + 2 * fs,
            lay.plot_w,
            self.fmax_of(cur),
            fs,
        );
        let wf_top = lay.top + lay.bars_h + lay.gap;
        let wf_h = s.h - wf_top - 2 * fs;
        self.draw_spectrogram(s, lay.left, wf_top, lay.plot_w, wf_h);
        s.rect(lay.left, wf_top, lay.plot_w, wf_h, PHOSPHOR, 60);
        let mut t = TextBuf::new();
        self.peak_text(cur, &mut t);
        if t.n == 0 {
            t.s(b"FFT 4096 PUNKTE  HANN  10.8 HZ/BIN");
        }
        s.text_buf(lay.left + 3 * fs, 3 * fs, &t, fs, PHOSPHOR);
    }

    /// The displayed bands as gradient bars plus their peak markers.
    fn draw_bars(&self, s: &Surface, lay: &Spec2Layout) {
        let bw = lay.plot_w as f64 / NB as f64;
        let base = lay.top + lay.bars_h - 1;
        let mut b = 0;
        while b < NB {
            let x0 = lay.left + (b as f64 * bw) as i32 + 1;
            let x1 = imax(lay.left + ((b + 1) as f64 * bw) as i32 - 1, x0);
            let hgt = (self.disp[b] as f64 * lay.bars_h as f64) as i32;
            let mut yy = 0;
            while yy < hgt {
                let t = (yy * 256 / imax(lay.bars_h, 1)) as u32;
                s.hline(x0, x1, base - yy, bar_color(t), 256);
                yy += 1;
            }
            if self.peak[b] > 0.01 {
                s.hline(
                    x0,
                    x1,
                    base - (self.peak[b] as f64 * lay.bars_h as f64) as i32,
                    WHITE,
                    220,
                );
            }
            b += 1;
        }
    }

    /// The spectrogram ring buffer scaled into the rectangle, newest column on the right.
    fn draw_spectrogram(&self, s: &Surface, x0: i32, y0: i32, w: i32, h: i32) {
        if h <= 0 {
            return;
        }
        let mut x = 0;
        while x < w {
            let kk = ((w - 1 - x) as usize * SG_W) / w as usize;
            let col = (self.sg_pos + SG_W - 1 - kk) % SG_W;
            let mut y = 0;
            while y < h {
                let row = y as usize * SG_H / h as usize;
                s.set(x0 + x, y0 + y, self.sg[row * SG_W + col]);
                y += 1;
            }
            x += 1;
        }
    }

    /// 3D spectrum: history rows recede into the depth, newest in front. Hidden lines
    /// are removed with a floating horizon (front to back), older rows fade out.
    pub(crate) fn draw_spec3(&mut self, s: &Surface, now: f64, cur: i32, labels: bool) {
        let fs = if labels { self.fs } else { 1 };
        let g = Spec3Geom {
            plot_w: (s.w - 12 * fs) as f64,
            cx: s.w as f64 / 2.0,
            y_front: (s.h - if labels { 12 * fs } else { 8 }) as f64,
            vp: s.h as f64 * 0.12,
            amp: s.h as f64 * 0.42,
        };
        let sw = imin(s.w, MAXW as i32) as usize;
        self.fill_horizon(sw, s.h);
        draw_floor(s, &g);
        let ref_t = if self.playing {
            now
        } else {
            self.hist_t[(self.hist_pos + HIST_N - 1) % HIST_N]
        };
        let mut n = 0;
        while n < HIST_N {
            let idx = (self.hist_pos + HIST_N - 1 - n) % HIST_N;
            let age = ref_t - self.hist_t[idx];
            if age > HIST_AGE {
                break;
            }
            self.draw_history_row(s, &g, idx, age, n == 0, sw);
            n += 1;
        }
        if labels {
            self.spec3_labels(s, &g, cur, fs);
        }
    }

    fn fill_horizon(&mut self, sw: usize, h: i32) {
        let mut x = 0;
        while x < sw {
            self.horizon[x] = h;
            x += 1;
        }
    }

    /// One history row `idx` of the given age as a hidden-line-removed polyline, coloured
    /// by loudness and faded by age. The front row gets a glow.
    fn draw_history_row(
        &mut self,
        s: &Surface,
        g: &Spec3Geom,
        idx: usize,
        age: f64,
        front: bool,
        sw: usize,
    ) {
        let z = clampf(age / HIST_AGE, 0.0, 1.0);
        let fade = pow(1.0 - z, 1.6);
        if fade < 0.02 {
            return;
        }
        let sc = 1.0 / (1.0 + z * PERSP);
        let base_y = g.vp + (g.y_front - g.vp) * sc;
        let mixv = clampf((self.row_loudness(idx) - 0.55) / 0.35, 0.0, 1.0);
        let col = dim(
            mix(PHOSPHOR, AMBER, (mixv * 256.0) as u32),
            (fade * 256.0) as u32,
        );
        copy_i32(&mut self.newh, &self.horizon, sw);
        let mut px = g.cx - g.plot_w * 0.5 * sc;
        let mut py = base_y;
        let mut b = 0;
        while b <= NB {
            let (qx, qy) = if b < NB {
                (
                    g.cx + ((b as f64 + 0.5) / NB as f64 - 0.5) * g.plot_w * sc,
                    base_y - self.hist[idx * NB + b] as f64 * g.amp * sc,
                )
            } else {
                (g.cx + g.plot_w * 0.5 * sc, base_y)
            };
            self.horizon_line(s, px, py, qx, qy, col, front);
            px = qx;
            py = qy;
            b += 1;
        }
        copy_i32(&mut self.horizon, &self.newh, sw);
    }

    /// Loudest band of history row `idx`.
    fn row_loudness(&self, idx: usize) -> f64 {
        let mut loud = 0.0;
        let mut b = 0;
        while b < NB {
            let v = self.hist[idx * NB + b] as f64;
            if v > loud {
                loud = v;
            }
            b += 1;
        }
        loud
    }

    /// Frequency labels, peak text and title of the big 3D view.
    fn spec3_labels(&self, s: &Surface, g: &Spec3Geom, cur: i32, fs: i32) {
        let pw = g.plot_w as i32;
        self.freq_labels(
            s,
            (g.cx - g.plot_w * 0.5) as i32,
            s.h - 9 * fs,
            pw,
            self.fmax_of(cur),
            fs,
        );
        let mut t = TextBuf::new();
        self.peak_text(cur, &mut t);
        s.text_buf(4 * fs, 3 * fs, &t, fs, PHOSPHOR);
        if (t.n as i32 + 18) * 6 * fs < s.w {
            s.text(s.w - 16 * 6 * fs, 3 * fs, b"3D  4 S VERLAUF", fs, PHOS_DIM);
        }
    }

    /// Draws a line from (x0, y0) to (x1, y1) only where it lies above the current
    /// horizon, and raises the next horizon (`newh`) accordingly.
    #[cfg(not(dos))]
    pub(crate) fn horizon_line(
        &mut self,
        s: &Surface,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        c: u32,
        front: bool,
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let n = (maxf(absf(dx), absf(dy)) as i32) + 1;
        let mut i = 0;
        while i <= n {
            let t = i as f64 / n as f64;
            let x = (x0 + dx * t) as i32;
            let y = (y0 + dy * t) as i32;
            if x >= 0 && x < s.w && (x as usize) < MAXW {
                let xu = x as usize;
                if y < self.horizon[xu] {
                    s.set(x, y, c);
                    if front {
                        s.glow(x, y - 1, c, 90);
                        s.glow(x, y + 1, c, 90);
                    }
                    if y < self.newh[xu] {
                        self.newh[xu] = y;
                    }
                }
            }
            i += 1;
        }
    }

    /// [`Vis::horizon_line`] for DOS: stepped in 16.16 fixed point instead of converting
    /// every pixel from floating point, which is slow on the Pentium's FPU.
    ///
    /// # Arguments
    /// * `s` - surface to draw on
    /// * `x0`, `y0` - start of the segment
    /// * `x1`, `y1` - end of the segment
    /// * `c` - colour
    /// * `front` - the front row (gets a glow)
    #[cfg(dos)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn horizon_line(
        &mut self,
        s: &Surface,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        c: u32,
        front: bool,
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let n = (maxf(absf(dx), absf(dy)) as i32) + 1;
        let mut fx = (x0 * 65536.0) as i32;
        let mut fy = (y0 * 65536.0) as i32;
        let sx = (dx * 65536.0) as i32 / n;
        let sy = (dy * 65536.0) as i32 / n;
        let mut i = 0;
        while i <= n {
            let x = fx >> 16;
            let y = fy >> 16;
            if x >= 0 && x < s.w && (x as usize) < MAXW {
                let xu = x as usize;
                if y < self.horizon[xu] {
                    s.set(x, y, c);
                    if front {
                        s.glow(x, y - 1, c, 90);
                        s.glow(x, y + 1, c, 90);
                    }
                    if y < self.newh[xu] {
                        self.newh[xu] = y;
                    }
                }
            }
            fx += sx;
            fy += sy;
            i += 1;
        }
    }
}

/// Geometry of the 3D spectrum: plot width, centre, front baseline, vanishing point and
/// amplitude of a full-scale band.
struct Spec3Geom {
    plot_w: f64,
    cx: f64,
    y_front: f64,
    vp: f64,
    amp: f64,
}

/// Five faint floor lines receding into the depth.
fn draw_floor(s: &Surface, g: &Spec3Geom) {
    let mut zi = 0;
    while zi <= 4 {
        let z = zi as f64 / 4.0;
        let sc = 1.0 / (1.0 + z * PERSP);
        let y = g.vp + (g.y_front - g.vp) * sc;
        let half = g.plot_w * 0.5 * sc;
        s.line(
            (g.cx - half) as f32,
            y as f32,
            (g.cx + half) as f32,
            y as f32,
            PHOSPHOR,
            26,
        );
        zi += 1;
    }
}

/// Horizontal dB lines every 24 dB with labels.
fn draw_db_grid(s: &Surface, lay: &Spec2Layout) {
    let fs = lay.fs;
    let mut db = -96;
    while db <= 0 {
        let y = lay.top + lay.bars_h - (db + 96) * lay.bars_h / 96;
        s.hline(lay.left, lay.left + lay.plot_w - 1, y, PHOSPHOR, 22);
        let mut t = TextBuf::new();
        if db < 0 {
            t.ch(45);
        }
        t.num((0 - db) as u32, 1, 48);
        s.text_buf(2 * fs, y - 3 * fs, &t, fs, PHOS_DIM);
        db += 24;
    }
}

/// Bar colour at height `t` (0..256): dim green, green, amber, red.
fn bar_color(t: u32) -> u32 {
    if t < 140 {
        mix(PHOS_DIM, PHOSPHOR, t * 256 / 140)
    } else if t < 218 {
        mix(PHOSPHOR, AMBER, (t - 140) * 256 / 78)
    } else {
        mix(AMBER, RED, u32min((t - 218) * 256 / 38, 256))
    }
}

/// Copies the first `n` values of `src` to `dst`.
fn copy_i32(dst: &mut [i32; MAXW], src: &[i32; MAXW], n: usize) {
    let mut i = 0;
    while i < n {
        dst[i] = src[i];
        i += 1;
    }
}

/// Spectrogram colour: black, deep green, phosphor green, amber, white.
pub(crate) fn wf_color(t: f64) -> u32 {
    let k = (clampf(t, 0.0, 1.0) * 1000.0) as u32;
    if k < 350 {
        mix(BLACK, rgb(0, 70, 30), k * 256 / 350)
    } else if k < 600 {
        mix(rgb(0, 70, 30), PHOSPHOR, (k - 350) * 256 / 250)
    } else if k < 820 {
        mix(PHOSPHOR, AMBER, (k - 600) * 256 / 220)
    } else {
        mix(AMBER, WHITE, (k - 820) * 256 / 180)
    }
}
