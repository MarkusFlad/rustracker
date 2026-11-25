//! Benchmark: shows every scene for a fixed time, measures frames per second and writes a
//! table. Shared by all editions: the framebuffer edition and the DOS edition let Rust
//! switch the scenes inside `Vis::frame`; the classic edition draws most views itself and
//! only uses the bookkeeping (`vis_bench_tick`, `vis_bench_view`) and the table.
//!
//! Per scene the first frame is measured separately ("1ST"): it includes the setup of the
//! view (the lake builds its landscape, the Mandelbrot zoom starts its first picture).
//! Then the frames of the following `secs` seconds give the average, the slowest
//! one-second window ("MIN"), the time per frame ("MS": the render time reported by the
//! page if it reports one, otherwise wall-clock time) and, for views with a Mandelbrot
//! picture, finished pictures per second ("PIC/S"). The score is the geometric mean of the
//! average frame rates, so a fast view does not hide a slow one.

use crate::gfx::*;
use crate::math::*;
use crate::vis::*;

/// Number of benchmark scenes.
pub(crate) const BENCH_N: usize = 8;
/// View of each scene.
const SC_VIEW: [u32; BENCH_N] = [
    V_OSC, V_SPEC, V_SPEC, V_MANDEL, V_CUBE, V_VOXEL, V_LAKE, V_DEMO,
];
/// Spectrum in 3D for each scene.
const SC_3D: [bool; BENCH_N] = [false, false, true, false, false, false, false, false];
/// Names, 12 characters each (the core-language subset copies no arrays of arrays).
pub(crate) const SC_NAMES: &[u8; 12 * BENCH_N] =
    b"OSCILLOSCOPESPECTRUM 2D SPECTRUM 3D MANDELBROT  CUBE        VOXEL       LAKE        DEMO SCENE  ";
/// Measuring time per scene in seconds when started with the B key.
pub(crate) const BENCH_SECS: f64 = 15.0;
/// Capacity of the result text.
pub(crate) const BENCH_TEXT: usize = 1024;

/// Benchmark not running and no result shown.
pub(crate) const BN_OFF: u32 = 0;
/// Benchmark running.
pub(crate) const BN_RUN: u32 = 1;
/// Benchmark finished, the result is shown.
pub(crate) const BN_DONE: u32 = 2;

impl Vis {
    /// Starts the benchmark with the first scene (the page restarts the song).
    ///
    /// # Arguments
    /// * `secs` - measuring time per scene in seconds (at least 1)
    pub fn bench_start(&mut self, secs: f64) {
        if self.bn_state != BN_RUN {
            self.bn_saved_view = self.view;
            self.bn_saved_3d = self.spec3d;
        }
        self.bn_state = BN_RUN;
        self.bn_secs = maxf(1.0, secs);
        self.bn_scene = 0;
        self.bn_n = 0;
        self.bn_cost = 0.0;
        self.bn_len = 0;
        self.bn_w = 0;
        self.bn_h = 0;
        let mut i = 0;
        while i < BENCH_N {
            self.bn_fps[i] = 0.0;
            self.bn_min[i] = 0.0;
            self.bn_ms[i] = 0.0;
            self.bn_first[i] = 0.0;
            self.bn_pic[i] = 0.0;
            i += 1;
        }
        self.bench_apply();
    }

    /// Stops a running benchmark or closes the result; the view from before comes back.
    pub fn bench_stop(&mut self) {
        if self.bn_state == BN_RUN {
            self.view = self.bn_saved_view;
            self.spec3d = self.bn_saved_3d;
            self.f_valid = false;
        }
        self.bn_state = BN_OFF;
    }

    /// State of the benchmark.
    ///
    /// # Returns
    /// 0 off, 1 running, 2 finished (result shown).
    pub fn bench_state(&self) -> u32 {
        self.bn_state
    }

    /// The scene whose frames are being measured right now (not its first frame).
    ///
    /// # Returns
    /// The scene index, or -1 (not running, or the first frame of a scene).
    pub fn bench_measuring(&self) -> i32 {
        if self.bn_state == BN_RUN && self.bn_n >= 2 {
            self.bn_scene as i32
        } else {
            -1
        }
    }

    /// The render time of the last frame, measured by the page (counts for "MS").
    ///
    /// # Arguments
    /// * `ms` - milliseconds; 0 if not measured
    pub fn bench_cost(&mut self, ms: f64) {
        self.bn_cost = ms;
    }

    /// The scene being measured as a view code.
    ///
    /// # Returns
    /// view number (as in `vis_view`) + 16 for the spectrum in 3D; 255 when not running.
    pub fn bench_view(&self) -> u32 {
        if self.bn_state != BN_RUN {
            return 255;
        }
        SC_VIEW[self.bn_scene] + if SC_3D[self.bn_scene] { 16 } else { 0 }
    }

    /// The result table (lines end with CR LF), empty until a benchmark has finished.
    ///
    /// # Returns
    /// Pointer to the bytes and their number.
    pub fn bench_text(&self) -> (*const u8, usize) {
        ((&raw const self.bn_text) as *const u8, self.bn_len)
    }

    /// Shows the scene `bn_scene`.
    fn bench_apply(&mut self) {
        self.view = SC_VIEW[self.bn_scene];
        self.spec3d = SC_3D[self.bn_scene];
        self.f_valid = false;
    }

    /// Called at the start of every frame while the benchmark runs: the frame before has
    /// just finished. Moves on to the next scene when the time is up.
    ///
    /// # Arguments
    /// * `now` - time in ms
    ///
    /// # Returns
    /// The scene index (0..8), or -1 when the benchmark is not running (any more).
    pub fn bench_tick(&mut self, now: f64) -> i32 {
        if self.bn_state != BN_RUN {
            return -1;
        }
        let cost = self.bn_cost;
        self.bn_cost = 0.0;
        if self.ended {
            self.bench_finish();
            return -1;
        }
        if !self.playing {
            self.bn_n = 0; // paused: measure the scene again once the music plays
            return self.bn_scene as i32;
        }
        if self.bn_n == 0 {
            self.bn_t0 = now;
            self.bn_n = 1;
            return self.bn_scene as i32;
        }
        if self.bn_n == 1 {
            // the first frame of the scene (with its setup) has finished
            self.bn_first[self.bn_scene] = now - self.bn_t0;
            self.bn_t1 = now;
            self.bn_win_t = now;
            self.bn_win_n = 0;
            self.bn_cost_sum = 0.0;
            self.bn_cost_n = 0;
            self.bn_worst = 1.0e9;
            self.bn_mb0 = self.mb_done;
            self.bn_n = 2;
            return self.bn_scene as i32;
        }
        self.bn_n += 1;
        self.bn_win_n += 1;
        if cost > 0.0 {
            self.bn_cost_sum += cost;
            self.bn_cost_n += 1;
        }
        if now - self.bn_win_t >= 1000.0 {
            let f = self.bn_win_n as f64 * 1000.0 / (now - self.bn_win_t);
            self.bn_worst = minf(self.bn_worst, f);
            self.bn_win_t = now;
            self.bn_win_n = 0;
        }
        if now - self.bn_t1 < self.bn_secs * 1000.0 {
            return self.bn_scene as i32;
        }
        // the scene is measured
        let i = self.bn_scene;
        let frames = (self.bn_n - 2) as f64;
        let span = now - self.bn_t1;
        self.bn_fps[i] = frames * 1000.0 / span;
        // the slowest full second (the average also counts the last, partial one)
        self.bn_min[i] = minf(self.bn_worst, self.bn_fps[i]);
        self.bn_ms[i] = if self.bn_cost_n > 0 {
            self.bn_cost_sum / self.bn_cost_n as f64
        } else {
            span / frames
        };
        let pics = if self.mb_done >= self.bn_mb0 {
            self.mb_done - self.bn_mb0
        } else {
            0
        };
        self.bn_pic[i] = pics as f64 * 1000.0 / span;
        self.bn_w = self.w;
        self.bn_h = self.h;
        if i + 1 < BENCH_N {
            self.bn_scene = i + 1;
            self.bn_n = 0;
            self.bench_apply();
            return self.bn_scene as i32;
        }
        self.bench_finish();
        -1
    }

    /// Ends the benchmark: writes the table, restores the view, shows the result.
    fn bench_finish(&mut self) {
        self.view = self.bn_saved_view;
        self.spec3d = self.bn_saved_3d;
        self.f_valid = false;
        self.bench_write();
        self.bn_state = BN_DONE;
    }

    /// Writes the result table into `bn_text`.
    fn bench_write(&mut self) {
        self.bn_len = 0;
        let mut t = TextBuf::new();
        t.s(b"RUSTRACKER BENCHMARK  ");
        t.num(self.bn_secs as u32, 1, 48);
        t.s(b" S PER SCENE  ");
        t.num(self.bn_w as u32, 1, 48);
        t.s(b" X ");
        t.num(self.bn_h as u32, 1, 48);
        self.bench_line(&t);
        t.clear();
        t.s(b"SCENE          FPS   MIN      MS    1ST  PIC/S");
        self.bench_line(&t);
        let mut log_sum = 0.0;
        let mut n = 0;
        let mut i = 0;
        while i < BENCH_N {
            t.clear();
            let mut k = 0;
            while k < 12 {
                t.ch(SC_NAMES[i * 12 + k]);
                k += 1;
            }
            if self.bn_fps[i] > 0.0 {
                rate(&mut t, self.bn_fps[i], 6);
                rate(&mut t, self.bn_min[i], 6);
                right(&mut t, self.bn_ms[i], 8, true);
                right(&mut t, self.bn_first[i], 7, false);
                if self.bn_pic[i] > 0.0 {
                    rate(&mut t, self.bn_pic[i], 7);
                } else {
                    t.s(b"      -");
                }
                log_sum += log2(self.bn_fps[i]);
                n += 1;
            } else {
                t.s(b"     -     -       -      -      -");
            }
            self.bench_line(&t);
            i += 1;
        }
        t.clear();
        t.s(b"SCORE (GEOMETRIC MEAN OF FPS) ");
        if n > 0 {
            let mut v = TextBuf::new();
            fps_text(&mut v, exp2(log_sum / n as f64));
            let mut k = 0;
            while k < v.n {
                t.ch(v.b[k]);
                k += 1;
            }
        } else {
            t.ch(45);
        }
        self.bench_line(&t);
    }

    /// Appends one line plus CR LF to `bn_text`.
    fn bench_line(&mut self, t: &TextBuf) {
        let mut k = 0;
        while k < t.n && self.bn_len + 2 < BENCH_TEXT {
            self.bn_text[self.bn_len] = t.b[k];
            self.bn_len += 1;
            k += 1;
        }
        if self.bn_len + 2 <= BENCH_TEXT {
            self.bn_text[self.bn_len] = 13;
            self.bn_text[self.bn_len + 1] = 10;
            self.bn_len += 2;
        }
    }

    /// Draws a small label into the view `v` while the benchmark runs, or the result table
    /// over the whole frame `s` when it has finished.
    pub(crate) fn draw_bench(&self, v: &Surface, s: &Surface) {
        let fs = self.fs;
        if self.bn_state == BN_RUN {
            let s = v;
            let mut t = TextBuf::new();
            t.s(b"BENCHMARK ");
            t.num(self.bn_scene as u32 + 1, 1, 48);
            t.ch(47);
            t.num(BENCH_N as u32, 1, 48);
            let w = (t.n as i32 + 2) * 6 * fs;
            let y = s.h - 12 * fs;
            s.fill_a(s.w - w, y, w, 12 * fs, BG, 200);
            s.text_buf(s.w - w + 6 * fs, y + 3 * fs, &t, fs, AMBER);
            return;
        }
        if self.bn_state != BN_DONE {
            return;
        }
        // the widest line has 47 characters, there are 12 lines
        let mut sc = fs;
        while sc > 1 && (49 * 6 * sc > s.w || 14 * 10 * sc > s.h) {
            sc -= 1;
        }
        let tw = 49 * 6 * sc;
        let th = 13 * 10 * sc;
        let x0 = imax(0, (s.w - tw) / 2);
        let y0 = imax(0, (s.h - th) / 2);
        s.fill_a(x0, y0, tw, th, BG, 235);
        s.rect(x0, y0, tw, th, PHOSPHOR, 160);
        let mut t = TextBuf::new();
        let mut line = 0;
        let mut k = 0;
        while k < self.bn_len {
            let c = self.bn_text[k];
            if c == 10 {
                let col = if line == 0 || line == 10 {
                    AMBER
                } else {
                    PHOSPHOR
                };
                s.text_buf(x0 + 6 * sc, y0 + (4 + line * 10) * sc, &t, sc, col);
                t.clear();
                line += 1;
            } else if c != 13 {
                t.ch(c);
            }
            k += 1;
        }
        t.clear();
        t.s(b"ANY KEY: CLOSE");
        s.text_buf(x0 + 6 * sc, y0 + (4 + 11 * 10) * sc, &t, sc, WHITE);
    }
}

/// Appends `v` with one decimal (two below 10), e.g. "42.5" or "0.35".
fn fps_text(t: &mut TextBuf, v: f64) {
    if v >= 10.0 {
        t.dec1(v);
    } else {
        let c = (v * 100.0 + 0.5) as u32;
        t.num(c / 100, 1, 48);
        t.ch(46);
        t.num(c % 100, 2, 48);
    }
}

/// Appends a frame rate right-aligned in `width` characters.
fn rate(t: &mut TextBuf, v: f64, width: usize) {
    let mut f = TextBuf::new();
    fps_text(&mut f, v);
    pad_copy(t, &f, width);
}

/// Appends a time in ms right-aligned in `width` characters, with one decimal or none.
fn right(t: &mut TextBuf, v: f64, width: usize, decimal: bool) {
    let mut f = TextBuf::new();
    if decimal {
        f.dec1(v);
    } else {
        f.num((v + 0.5) as u32, 1, 48);
    }
    pad_copy(t, &f, width);
}

/// Appends `f` right-aligned in `width` characters.
fn pad_copy(t: &mut TextBuf, f: &TextBuf, width: usize) {
    let mut n = f.n;
    while n < width {
        t.ch(32);
        n += 1;
    }
    let mut k = 0;
    while k < f.n {
        t.ch(f.b[k]);
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_every_scene_and_writes_a_table() {
        static mut V: Vis = Vis::new();
        let v = unsafe { &mut *(&raw mut V) };
        v.playing = true;
        v.view = V_LAKE;
        v.bench_start(2.0);
        assert_eq!(v.bench_view(), V_OSC);
        // scene i: first frame 100 ms, then frames of (i + 1) * 10 ms
        let mut now = 0.0;
        let mut guard = 0;
        while v.bench_state() == BN_RUN && guard < 100_000 {
            let sc = v.bench_tick(now);
            if sc == 2 {
                assert_eq!(v.bench_view(), V_SPEC + 16);
            }
            let first = v.bn_n == 1;
            now += if first {
                100.0
            } else {
                (v.bn_scene + 1) as f64 * 10.0
            };
            guard += 1;
        }
        assert_eq!(v.bench_state(), BN_DONE);
        assert_eq!(v.view, V_LAKE); // the view from before is back
        for i in 0..BENCH_N {
            let want = 100.0 / (i + 1) as f64;
            assert!(
                (v.bn_fps[i] - want).abs() < want * 0.02,
                "scene {i}: {}",
                v.bn_fps[i]
            );
            assert!((v.bn_first[i] - 100.0).abs() < 1e-9);
        }
        let (p, n) = v.bench_text();
        let text = unsafe { std::slice::from_raw_parts(p, n) };
        let text = std::str::from_utf8(text).unwrap();
        assert!(
            text.contains("OSCILLOSCOPE 100.0 100.0    10.0    100      -"),
            "{text}"
        );
        assert!(text.contains("DEMO SCENE    12.5  12.5    80.0"), "{text}");
        assert_eq!(text.lines().count(), 11);
        v.bench_stop();
        assert_eq!(v.bench_state(), BN_OFF);
    }
}
