//! Voxel view: the Mandelbrot picture of the music director as a landscape.
//!
//! Every finished Mandelbrot buffer becomes a height map: points of the set lie at height
//! zero, the fewer iterations a point needs to escape, the higher it is (histogram-equalised
//! over the picture, so the heights spread evenly and the outer regions rise to peaks). Each
//! voxel keeps the colour of the 2D picture, lit by its slope. A camera orbits the landscape
//! and looks past its centre, so it flies around the peaks; the zoom of the director makes
//! the landscape grow. Rendering: [`crate::voxel_space`].

use crate::gfx::*;
use crate::math::*;
use crate::vis::*;
use crate::vis_cube::draw_vertical_gradient;
use crate::voxel_space::{render, HeightMap, Orbit, FOG, HSCALE, NO_SHORE};

/// Half the width of the landscape in world units (the director's picture spans ±1; the
/// buffer has some margin, which the zoom makes grow).
const EXTENT: f64 = 1.6;
/// Bins of the iteration histogram: one per iteration (the palette index has 16 steps
/// per iteration); more iterations share the last bin.
pub(crate) const HIST_BINS: usize = 4096;
/// The camera orbits the centre once a minute over the landscape, breathing in and out a
/// little, and looks past the centre (to the left of it), so it circles around the peaks.
const ORBIT: Orbit = Orbit {
    speed: 0.105,
    radius: 0.85,
    breathe: 0.15,
    breathe_rate: 0.23,
    height: 0.85,
    rise: 0.06,
    rise_rate: 0.17,
    rise_phase: 0.0,
    look: 0.55,
    horizon: 0.24,
    aim: 0.0,
    aim_height: 0.0,
};

impl Vis {
    /// The voxel view: keeps the square Mandelbrot buffer coming (like the cube face),
    /// turns new pictures into height maps and renders the landscape from an orbiting camera.
    pub(crate) fn draw_voxel(&mut self, s: &Surface, now: f64, dt: f64) {
        self.keep_face_mandel_going();
        self.pulse *= exp_neg(dt * 6.0);
        draw_vertical_gradient(s, BG, FOG);
        if !self.f_valid {
            self.vx_ready = false;
            return;
        }
        if self.mb_done != self.vx_seen || !self.vx_ready {
            self.build_heights();
        }
        self.build_colours();
        let cam = ORBIT.camera((now - self.t0) / 1000.0, 0.04 * self.pulse, s.h);
        let map = self.voxel_map();
        let flash = (clampf(self.d_flash, 0.0, 0.6) * 256.0) as u32;
        unsafe { render(s, &cam, &map, flash, self.vx_fine) };
        self.draw_mandel_hud(s);
    }

    /// Height map and lighting of the front buffer. Heights are histogram-equalised:
    /// a point's height is the share of outside points that need more iterations than it,
    /// so the points with the fewest iterations form the peaks (1), the set and its
    /// boundary lie at 0, and the heights spread evenly over the whole landscape.
    fn build_heights(&mut self) {
        let front = self.mb_front * MB_MAX;
        let n = (self.f_w * self.f_h) as usize;
        let outside = self.build_histogram(front, n);
        let mut i = 0;
        while i < n {
            let v = self.mb[front + i];
            self.vx_h[i] = if v == 0xffff_ffff || outside == 0 {
                0
            } else {
                let b = hist_bin(v);
                let below =
                    self.vx_cdf[b] as f64 + self.vx_hist[b] as f64 * ((v & 15) as f64 / 16.0);
                ((1.0 - below / outside as f64) * HSCALE) as u16
            };
            i += 1;
        }
        self.build_light();
        self.vx_seen = self.mb_done;
        self.vx_ready = true;
    }

    /// Histogram of the outside points per iteration (`vx_hist`) and the number of points
    /// in all lower bins (`vx_cdf`). Returns the number of outside points.
    fn build_histogram(&mut self, front: usize, n: usize) -> u32 {
        let mut b = 0;
        while b < HIST_BINS {
            self.vx_hist[b] = 0;
            b += 1;
        }
        let mut outside = 0;
        let mut i = 0;
        while i < n {
            let v = self.mb[front + i];
            if v != 0xffff_ffff {
                self.vx_hist[hist_bin(v)] += 1;
                outside += 1;
            }
            i += 1;
        }
        let mut sum = 0;
        b = 0;
        while b < HIST_BINS {
            self.vx_cdf[b] = sum;
            sum += self.vx_hist[b];
            b += 1;
        }
        outside
    }

    /// Brightness per voxel (0..255) from the slope of the height map: light comes from
    /// the upper left of the picture.
    fn build_light(&mut self) {
        let (w, h) = (self.f_w, self.f_h);
        let mut y = 0;
        while y < h {
            let mut x = 0;
            while x < w {
                let gx = self.height_at(x + 1, y) - self.height_at(x - 1, y);
                let gy = self.height_at(x, y + 1) - self.height_at(x, y - 1);
                let l = clampf(170.0 - (gx + gy) * 0.45, 60.0, 255.0);
                self.vx_l[(y * w + x) as usize] = l as u8;
                x += 1;
            }
            y += 1;
        }
    }

    /// Stored height at buffer pixel (x, y), clamped to the edges.
    fn height_at(&self, x: i32, y: i32) -> f64 {
        let x = iclamp(x, 0, self.f_w - 1);
        let y = iclamp(y, 0, self.f_h - 1);
        self.vx_h[(y * self.f_w + x) as usize] as f64
    }

    /// Lit colour of every voxel for this frame: the 2D palette colour with the current
    /// palette shift (black inside the set), dimmed by the slope light.
    fn build_colours(&mut self) {
        let front = self.mb_front * MB_MAX;
        let shift = (self.d_shift_shown as i64 % PAL_LEN as i64) as u32;
        let n = (self.f_w * self.f_h) as usize;
        let mut i = 0;
        while i < n {
            let v = self.mb[front + i];
            let base = if v == 0xffff_ffff {
                BLACK
            } else {
                self.pal[((v + shift) % PAL_LEN as u32) as usize]
            };
            self.vx_c[i] = dim(base, self.vx_l[i] as u32 + 1);
            i += 1;
        }
    }

    /// The landscape: world ±1 is the current view of the director, mapped onto the front
    /// buffer. Using the current view instead of the buffer's own makes the zoom continuous.
    fn voxel_map(&self) -> HeightMap {
        let half = self.v_size * 0.5;
        HeightMap {
            heights: (&raw const self.vx_h) as *const u16,
            colours: (&raw const self.vx_c) as *const u32,
            w: self.f_w,
            h: self.f_h,
            x0: (self.v_cx - self.f_re0) / self.f_step,
            y0: (self.f_im0 - self.v_cy) / self.f_step,
            k: half / self.f_step,
            extent: EXTENT,
            shore: NO_SHORE,
        }
    }
}

/// Histogram bin of a palette index.
#[inline(always)]
fn hist_bin(v: u32) -> usize {
    let b = (v >> 4) as usize;
    if b < HIST_BINS {
        b
    } else {
        HIST_BINS - 1
    }
}
