//! Voxel view: the Mandelbrot picture of the music director as a landscape.
//!
//! Every finished Mandelbrot buffer becomes a height map: points of the set lie at height
//! zero, the fewer iterations a point needs to escape, the higher it is (histogram-equalised
//! over the picture, so the heights spread evenly and the outer regions rise to peaks). Each voxel keeps the colour of the 2D picture, lit by its
//! slope. A camera orbits the landscape and looks past its centre, so it flies around the
//! peaks; the zoom of the director makes the landscape grow.
//!
//! Rendering is the classic voxel-space algorithm: one ray per screen column, marched front
//! to back over the height map, each sample drawn as a vertical span above everything nearer.

use crate::gfx::*;
use crate::math::*;
use crate::vis::*;
use crate::vis_cube::draw_vertical_gradient;

/// Half the width of the landscape in world units (the director's picture spans ±1; the
/// buffer has some margin, which the zoom makes grow).
const EXTENT: f64 = 1.6;
/// World height of the highest voxels.
const HMAX: f64 = 0.55;
/// Height map resolution: stored heights run from 0 to this value.
const HSCALE: f64 = 1000.0;
/// Farthest distance a ray travels.
const ZMAX: f64 = 2.6;
/// Bins of the iteration histogram: one per iteration (the palette index has 16 steps
/// per iteration); more iterations share the last bin.
pub(crate) const HIST_BINS: usize = 4096;
/// Sky colour at the horizon; distant voxels fade into it.
const FOG: u32 = rgb(16, 34, 27);

/// Camera of one frame: position, viewing direction and projection.
struct Camera {
    x: f64,
    y: f64,
    z: f64,
    fx: f64,
    fy: f64,
    rx: f64,
    ry: f64,
    focal: f64,
    horizon: f64,
}

/// Maps world coordinates onto the front buffer of the Mandelbrot renderer.
struct MapXform {
    x0: f64,
    y0: f64,
    k: f64,
}

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
        let cam = self.camera(s, now);
        let map = self.map_xform();
        let shift = (self.d_shift_shown as i64 % PAL_LEN as i64) as u32;
        let flash = (clampf(self.d_flash, 0.0, 0.6) * 256.0) as u32;
        let mut col = 0;
        while col < s.w {
            self.voxel_column(s, &cam, &map, col, shift, flash);
            col += 1;
        }
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

    /// The camera orbits the centre once a minute over the landscape, breathing in and out
    /// a little, and looks past the centre (to the left of it) so that it circles around
    /// the peaks.
    /// Beats push it down slightly.
    fn camera(&self, s: &Surface, now: f64) -> Camera {
        let t = (now - self.t0) / 1000.0;
        let ang = t * 0.105;
        let r = 0.85 + 0.15 * sin(t * 0.23);
        let yaw = ang + PI + 0.55;
        let (fx, fy) = (cos(yaw), sin(yaw));
        Camera {
            x: r * cos(ang),
            y: r * sin(ang),
            z: 0.85 + 0.06 * sin(t * 0.17) - 0.04 * self.pulse,
            fx,
            fy,
            rx: fy,
            ry: 0.0 - fx,
            focal: s.h as f64 * 0.85,
            horizon: s.h as f64 * 0.24,
        }
    }

    /// World (±1 = the current view of the director) to front-buffer pixels. Using the
    /// current view instead of the buffer's own makes the zoom continuous.
    fn map_xform(&self) -> MapXform {
        let half = self.v_size * 0.5;
        MapXform {
            x0: (self.v_cx - self.f_re0) / self.f_step,
            y0: (self.f_im0 - self.v_cy) / self.f_step,
            k: half / self.f_step,
        }
    }

    /// Marches the ray of screen column `col` from near to far and draws every voxel that
    /// rises above what is already drawn in this column.
    fn voxel_column(
        &self,
        s: &Surface,
        cam: &Camera,
        map: &MapXform,
        col: i32,
        shift: u32,
        flash: u32,
    ) {
        let off = (col as f64 - s.w as f64 / 2.0 + 0.5) / cam.focal;
        let dx = cam.fx + cam.rx * off;
        let dy = cam.fy + cam.ry * off;
        let front = self.mb_front * MB_MAX;
        let mut ybuf = s.h;
        let mut z = 0.06;
        while z < ZMAX && ybuf > 0 {
            let wx = cam.x + dx * z;
            let wy = cam.y + dy * z;
            let idx = self.map_index(map, wx, wy);
            if idx >= 0 {
                let i = idx as usize;
                let hh = self.vx_h[i] as f64 * (HMAX / HSCALE);
                let sy = (cam.horizon + (cam.z - hh) * cam.focal / z) as i32;
                if sy < ybuf {
                    let c = self.voxel_color(front + i, i, z, shift, flash);
                    fill_span(s, col, imax(sy, 0), ybuf, c);
                    ybuf = sy;
                }
            }
            z += z * 0.011 + 0.0015;
        }
    }

    /// Buffer index of world point (wx, wy), or -1 outside the landscape or the buffer.
    #[inline(always)]
    fn map_index(&self, map: &MapXform, wx: f64, wy: f64) -> i32 {
        if wx < 0.0 - EXTENT || wx > EXTENT || wy < 0.0 - EXTENT || wy > EXTENT {
            return -1;
        }
        let bx = map.x0 + wx * map.k;
        let by = map.y0 - wy * map.k;
        if bx < 0.0 || by < 0.0 {
            return -1;
        }
        let (x, y) = (bx as i32, by as i32);
        if x >= self.f_w || y >= self.f_h {
            return -1;
        }
        y * self.f_w + x
    }

    /// Colour of a voxel: the 2D palette colour (black inside the set), lit, faded into
    /// the fog with distance `z` and brightened by the beat flash.
    #[inline(always)]
    fn voxel_color(&self, mb_i: usize, i: usize, z: f64, shift: u32, flash: u32) -> u32 {
        let v = self.mb[mb_i];
        let base = if v == 0xffff_ffff {
            BLACK
        } else {
            self.pal[((v + shift) % PAL_LEN as u32) as usize]
        };
        let lit = dim(base, self.vx_l[i] as u32 + 1);
        let fog = z / ZMAX;
        let c = mix(lit, FOG, (fog * fog * 230.0) as u32);
        if flash > 0 {
            mix(c, WHITE, flash)
        } else {
            c
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

/// Fills column `x` from row `y0` up to (not including) `y1`.
#[inline(always)]
fn fill_span(s: &Surface, x: i32, y0: i32, y1: i32, c: u32) {
    let mut p = s.p as usize + ((y0 * s.stride + x) as usize) * 4;
    let step = s.stride as usize * 4;
    let mut y = y0;
    while y < y1 {
        unsafe { *(p as *mut u32) = c };
        p += step;
        y += 1;
    }
}
