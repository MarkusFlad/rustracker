//! Mandelbrot view: music director, buffers rendered by the SIMD kernel in Web Workers (or
//! by the built-in scalar path), and the rotated, bilinear reprojection onto the screen.

use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::vis::*;

impl Vis {
    /// Builds the colour table of palette `pal_idx` from its six stops, with smoothstep
    /// interpolation between neighbouring stops.
    pub(crate) fn make_palette(&mut self) {
        let base = (self.pal_idx * 6) as usize;
        let mut i = 0;
        while i < PAL_LEN {
            let t = (i * 1000 / PAL_LEN) as u32;
            let mut k = 0;
            while k < 4 && PAL_POS[base + k + 1] < t {
                k += 1;
            }
            let t0 = PAL_POS[base + k];
            let t1 = PAL_POS[base + k + 1];
            let mut f = (t - t0) as f64 / (t1 - t0) as f64;
            f = f * f * (3.0 - 2.0 * f);
            self.pal[i] = mix(PAL_COL[base + k], PAL_COL[base + k + 1], (f * 256.0) as u32);
            i += 1;
        }
    }

    /// Music director: one journey per order position, a zoom push per beat, palette shifts
    /// on notes, flashes on bars, panning and swaying in time.
    pub(crate) fn direct(&mut self, now: f64, dt: f64, cur: i32, pl: &Player) {
        self.d_flash *= exp_neg(dt * 7.0);
        if cur < 0 {
            return;
        }
        let m = self.msgs[cur as usize].copy();
        let row_f = self.row_position(&m, now);
        if m.order as i32 != self.d_order {
            self.start_journey(m.order as i32);
        }
        let e = beat_eased(row_f / 4.0);
        self.advance_zoom(e);
        let r = row_f as u32;
        let key = m.order * 64 + r;
        if key != self.d_rowkey {
            self.d_rowkey = key;
            self.on_new_row(pl, m.pattern as usize, r);
        }
        self.d_shift_shown += (self.d_shift - self.d_shift_shown) * minf(1.0, dt * 10.0);
        self.sway(e, row_f);
    }

    /// The current row with its fraction, interpolated from the message time while playing.
    fn row_position(&self, m: &Msg, now: f64) -> f64 {
        if !self.playing {
            return m.row as f64;
        }
        let row_dur = 2.5 * m.speed as f64 / m.tempo as f64 * 1000.0;
        minf(
            m.row as f64 + maxf(0.0, now - m.t) / row_dur,
            minf(m.row as f64 + 1.999, 63.999),
        )
    }

    /// A new order position: next place, zoom out, flash.
    fn start_journey(&mut self, order: i32) {
        self.d_order = order;
        self.d_place = (self.d_place + 1) % 5;
        self.d_z = 0.0;
        self.d_has_e = false;
        self.d_flash = maxf(self.d_flash, 0.45);
        self.f_valid = false;
    }

    /// Zooms in by the eased beat progress `e`, faster when the music is loud, but not
    /// beyond the depth the place's coordinates allow.
    fn advance_zoom(&mut self, e: f64) {
        if !self.d_has_e || e < self.d_prev_e {
            self.d_prev_e = e;
            self.d_has_e = true;
        }
        let rate = 0.3 + 0.8 * self.inten;
        self.d_z = minf(
            self.d_z + (e - self.d_prev_e) * rate,
            log2(3.0 / PLACE_MIN[self.d_place]) - 0.3,
        );
        self.d_prev_e = e;
    }

    /// Reacts to row `r` of `pattern`: palette shift per note, flashes and cube pulses
    /// on beats and bars.
    fn on_new_row(&mut self, pl: &Player, pattern: usize, r: u32) {
        let mut hits = 0;
        let mut c = 0;
        while c < pl.channels {
            if pl.cell(pattern, r as usize, c).period != 0 {
                hits += 1;
            }
            c += 1;
        }
        self.d_shift += hits as f64 * 26.0 * (0.5 + self.inten);
        if r % 16 == 0 {
            self.d_flash = maxf(self.d_flash, 0.12 + 0.3 * self.inten);
        } else if r % 4 == 0 && hits > 0 {
            self.d_flash = maxf(self.d_flash, 0.1 * self.inten);
        }
        if r % 4 == 0 {
            self.pulse = if r % 16 == 0 { 1.0 } else { 0.55 };
        }
    }

    /// Circles around the place once per four beats and sways the rotation per bar.
    fn sway(&mut self, e: f64, row_f: f64) {
        let place = self.d_place;
        let size = 3.0 * exp2(0.0 - self.d_z);
        let ang = e * PI / 2.0;
        let rad = size * 0.05 * (0.4 + self.inten);
        self.v_rot = 0.14 * (0.4 + 0.6 * self.inten) * sin(PI * beat_eased(row_f / 16.0));
        self.v_cx = PLACE_CX[place] + rad * cos(ang);
        self.v_cy = PLACE_CY[place] + rad * sin(ang);
        self.v_size = size;
    }

    /// Iteration limit for the current zoom depth (a multiple of 4 for the SIMD kernel).
    pub(crate) fn max_iter(&self) -> u32 {
        let z = maxf(1.0, 3.0 / self.v_size);
        let l = log2(z);
        let it = minf(3000.0, maxf(64.0, 120.0 + 60.0 * pow(maxf(l, 0.0), 1.15)));
        ((it as u32) / 4) * 4
    }

    /// Start rendering a new back buffer for the current view (overscan for rotation).
    pub(crate) fn mb_start(&mut self, vw: i32, vh: i32, face: bool) {
        let mut f = if face {
            1.0
        } else if self.mb_factor < 1.0 {
            2.0
        } else {
            self.mb_factor
        };
        let mut bw;
        let mut bh;
        loop {
            bw = (((vw as f64 * 1.3 / f) as i32 + 8) / 8) * 8;
            bh = (vh as f64 * 1.3 / f) as i32 + 1;
            if (bw as usize) * (bh as usize) <= MB_MAX {
                break;
            }
            f += 0.25;
        }
        if !face {
            self.mb_factor = f;
        }
        self.b_face = face;
        let step = self.v_size * 1.3 / bh as f64;
        self.b_w = bw;
        self.b_h = bh;
        self.b_step = step;
        self.b_re0 = self.v_cx - (bw as f64 / 2.0 - 0.5) * step;
        self.b_im0 = self.v_cy + (bh as f64 / 2.0 - 0.5) * step;
        self.b_row = 0;
        self.b_frames = 0;
        self.b_maxit = self.max_iter();
        self.b_active = true;
        self.job_gen += 1;
        self.job_taken = false;
        self.job_rows = 0;
    }

    /// Work on the back buffer for up to `budget` iterations; swap when complete.
    /// With Web Workers the rows are rendered there instead (see the job protocol).
    pub(crate) fn mb_work(&mut self, budget: u64) {
        self.b_frames += 1;
        if self.workers {
            return;
        }
        let back = (1 - self.mb_front) * MB_MAX;
        let mut used: u64 = 0;
        while self.b_row < self.b_h && used < budget {
            let ci = self.b_im0 - self.b_row as f64 * self.b_step;
            let rowbase = back + (self.b_row * self.b_w) as usize;
            let mut x = 0;
            while x < self.b_w {
                let (v, n) = escape_index(self.b_re0 + x as f64 * self.b_step, ci, self.b_maxit);
                used += n as u64 + 4;
                self.mb[rowbase + x as usize] = v;
                x += 1;
            }
            self.b_row += 1;
        }
        if self.b_row >= self.b_h {
            self.mb_finish();
        }
    }

    /// The back buffer is complete: show it and adapt the resolution.
    pub(crate) fn mb_finish(&mut self) {
        self.mb_front = 1 - self.mb_front;
        self.f_re0 = self.b_re0;
        self.f_im0 = self.b_im0;
        self.f_step = self.b_step;
        self.f_w = self.b_w;
        self.f_h = self.b_h;
        self.f_valid = true;
        self.b_active = false;
        // adapt the resolution: aim at a new picture every ~3 frames
        if !self.b_face && self.b_frames > 8 {
            self.mb_factor = minf(6.0, self.mb_factor + 0.25);
        } else if !self.b_face && self.b_frames <= 3 {
            self.mb_factor = maxf(1.0, self.mb_factor - 0.25);
        }
    }

    /// Show the newest finished buffer, mapped into the current (rotated, zoomed) view.
    pub(crate) fn mb_blit(&self, s: &Surface, rot: f64, flash: f64) {
        if !self.f_valid {
            return;
        }
        let front = self.mb_front * MB_MAX;
        let sc = self.v_size / s.h as f64;
        let co = cos(rot);
        let si = sin(rot);
        // complex point of screen (x, y): affine in x and y -> buffer coordinates
        let fx0 = (self.v_cx - self.f_re0) / self.f_step;
        let fy0 = (self.f_im0 - self.v_cy) / self.f_step;
        let ax = co * sc / self.f_step; // d fx / dx
        let ay = si * sc / self.f_step; // d fx / dy
        let bx = 0.0 - si * sc / self.f_step; // d fy / dx  (screen y down = im down)
        let by = co * sc / self.f_step;
        let shift = (self.d_shift_shown as i64 % PAL_LEN as i64) as u32;
        let fl = (clampf(flash, 0.0, 0.6) * 256.0) as u32;
        let hw = s.w as f64 / 2.0 - 0.5;
        let hh = s.h as f64 / 2.0 - 0.5;
        let mut y = 0;
        while y < s.h {
            let dy = y as f64 - hh;
            let mut fx = fx0 + ax * (0.0 - hw) + ay * dy + 0.5;
            let mut fy = fy0 + bx * (0.0 - hw) + by * dy + 0.5;
            let mut x = 0;
            while x < s.w {
                let mut c = self.mb_bilinear(front, fx - 0.5, fy - 0.5, shift);
                if fl > 0 {
                    c = mix(c, WHITE, fl);
                }
                s.set(x, y, c);
                fx += ax;
                fy += bx;
                x += 1;
            }
            y += 1;
        }
    }

    /// Colour at buffer position (u, v): blend of the four neighbouring buffer pixels.
    #[inline(always)]
    fn mb_bilinear(&self, front: usize, u: f64, v: f64, shift: u32) -> u32 {
        let i0 = floor(u);
        let j0 = floor(v);
        let tx = ((u - i0) * 256.0) as u32;
        let ty = ((v - j0) * 256.0) as u32;
        let (ix, iy) = (i0 as i32, j0 as i32);
        let c00 = self.mb_color(front, ix, iy, shift);
        let c10 = self.mb_color(front, ix + 1, iy, shift);
        let c01 = self.mb_color(front, ix, iy + 1, shift);
        let c11 = self.mb_color(front, ix + 1, iy + 1, shift);
        mix(mix(c00, c10, tx), mix(c01, c11, tx), ty)
    }

    // ---- job protocol for the Web Workers running the SIMD kernel
    //
    // JS asks `mb_take` for a job; when it gets a generation, it reads the parameters
    // with `mb_param`, lets the workers render row bands into their own memory, copies
    // finished rows to `mb_back_ptr` and reports them with `mb_rows`. A newer generation
    // (new view) makes late results of an older one be ignored.

    /// Switches between the Web Workers (`n` > 0) and the built-in scalar renderer.
    pub fn mb_set_workers(&mut self, n: i32) {
        self.workers = n > 0;
        self.b_active = false;
    }
    /// Returns the generation of a job that still has to be handed out (0 = none).
    pub fn mb_take(&mut self) -> u32 {
        if self.workers && self.b_active && !self.job_taken {
            self.job_taken = true;
            return self.job_gen;
        }
        0
    }
    /// Generation of the current job.
    pub fn mb_gen(&self) -> u32 {
        self.job_gen
    }
    /// Job parameter `i`: 0 re0 (left), 1 im0 (top), 2 step per pixel, 3 width (= stride),
    /// 4 height, 5 maximum iterations.
    pub fn mb_param(&self, i: i32) -> f64 {
        if i == 0 {
            self.b_re0
        } else if i == 1 {
            self.b_im0
        } else if i == 2 {
            self.b_step
        } else if i == 3 {
            self.b_w as f64
        } else if i == 4 {
            self.b_h as f64
        } else {
            self.b_maxit as f64
        }
    }
    /// Where the rows of the current job go (palette indices, `0xffff_ffff` = inside).
    pub fn mb_back_ptr(&mut self) -> *mut u32 {
        ((&raw mut self.mb) as usize + (1 - self.mb_front) * MB_MAX * 4) as *mut u32
    }
    /// `rows` rows of job `gen` have been written into the back buffer.
    pub fn mb_rows(&mut self, gen: u32, rows: i32) {
        if gen != self.job_gen || !self.b_active {
            return;
        }
        self.job_rows += rows;
        if self.job_rows >= self.b_h {
            self.mb_finish();
        }
    }

    /// Colour of front buffer pixel (x, y) with the palette rotated by `shift`; black
    /// outside the buffer and inside the set.
    pub(crate) fn mb_color(&self, front: usize, x: i32, y: i32, shift: u32) -> u32 {
        if x < 0 || y < 0 || x >= self.f_w || y >= self.f_h {
            return BLACK;
        }
        let v = self.mb[front + (y * self.f_w + x) as usize];
        if v == 0xffff_ffff {
            BLACK
        } else {
            self.pal[((v + shift) % PAL_LEN as u32) as usize]
        }
    }

    /// The Mandelbrot view: keeps the renderer going, shows the newest picture rotated
    /// and flashing in time, and with `hud` the place, zoom, resolution and level.
    pub(crate) fn draw_mandel(&mut self, s: &Surface, hud: bool) {
        if self.b_active && self.b_face {
            self.b_active = false; // the cube face buffer has the wrong size
        }
        if !self.b_active {
            self.mb_start(s.w, s.h, false);
        }
        self.mb_work(3_500_000);
        let rot = self.v_rot;
        let fl = self.d_flash;
        self.mb_blit(s, rot, fl);
        if hud {
            self.draw_mandel_hud(s);
        }
    }

    /// Info line in the top right corner; the place name is dropped if it does not fit.
    fn draw_mandel_hud(&self, s: &Surface) {
        let fs = self.fs;
        let mut t = TextBuf::new();
        place_name(&mut t, self.d_place);
        t.s(b"  ZOOM ");
        zoom_text(&mut t, 3.0 / self.v_size);
        t.s(b"  1/");
        t.dec1(self.mb_factor);
        t.s(b"  PEGEL ");
        t.num((self.inten * 100.0) as u32, 1, 48);
        t.ch(37);
        if t.n as i32 * 6 * fs > s.w - 8 * fs {
            drop_first_word(&mut t);
        }
        let tw = t.n as i32 * 6 * fs;
        s.fill_a(s.w - tw - 6 * fs, 2 * fs, tw + 4 * fs, 10 * fs, BLACK, 150);
        s.text_buf(s.w - tw - 4 * fs, 4 * fs, &t, fs, WHITE);
    }
}

/// Appends the name of place `p` of the journey.
fn place_name(t: &mut TextBuf, p: usize) {
    if p == 0 {
        t.s(b"SEEPFERDCHENTAL");
    } else if p == 1 {
        t.s(b"DOPPELSPIRALE");
    } else if p == 2 {
        t.s(b"ELEFANTENTAL");
    } else if p == 3 {
        t.s(b"SPIRALE");
    } else {
        t.s(b"MINI-MANDELBROT");
    }
}

/// Removes everything up to and including the first double space.
fn drop_first_word(t: &mut TextBuf) {
    let mut k = 0;
    while k < t.n && t.b[k] != 32 {
        k += 1;
    }
    let mut j = 0;
    while k + 2 + j < t.n {
        t.b[j] = t.b[k + 2 + j];
        j += 1;
    }
    t.n = j;
}

/// Escape-time iteration of c = cr + i·ci. Returns the smooth palette index
/// (16 steps per iteration, `0xffff_ffff` inside the set) and the iterations used.
#[inline(always)]
fn escape_index(cr: f64, ci: f64, maxit: u32) -> (u32, u32) {
    let mut zr = cr;
    let mut zi = ci;
    let mut n = 0;
    let mut mag = 0.0;
    while n < maxit {
        let x2 = zr * zr;
        let y2 = zi * zi;
        mag = x2 + y2;
        if mag > 4.0 {
            break;
        }
        zi = 2.0 * zr * zi + ci;
        zr = x2 - y2 + cr;
        n += 1;
    }
    if n >= maxit {
        return (0xffff_ffff, n);
    }
    let nu = maxf(0.0, n as f64 + 1.0 - log2(log2(mag) * 0.5));
    ((nu * 16.0) as u32, n)
}

/// Whole beats plus the eased fraction of the current one (fast start, soft landing).
fn beat_eased(beat: f64) -> f64 {
    floor(beat) + ease_out(fract(beat))
}

/// Zoom factor as text: "123X" or "1.2E5X".
pub(crate) fn zoom_text(t: &mut TextBuf, z: f64) {
    if z < 10000.0 {
        t.num((z + 0.5) as u32, 1, 48);
    } else {
        let e = floor(log10(z));
        let m = z / pow(10.0, e);
        t.dec1(m);
        t.s(b"E");
        t.num(e as u32, 1, 48);
    }
    t.ch(88);
}

/// Quartic ease-out: 0 -> 0, 1 -> 1, steep at the start.
pub(crate) fn ease_out(p: f64) -> f64 {
    let q = 1.0 - p;
    1.0 - q * q * q * q
}
