//! Reusable voxel-space renderer for height-map landscapes (Mandelbrot landscape, lake).
//!
//! The classic algorithm: one ray per screen column, marched front to back over the height
//! map; every sample that rises above what is already drawn in the column is drawn as a
//! vertical span down to the previous top. Each map cell has a height and a (lit) colour;
//! distance fades colours into the fog, and a flash brightens everything. In the fine mode
//! heights and colours are interpolated bilinearly between cells and the rays take smaller
//! steps, so the blocky voxels become a smooth surface. A map can also have a [`Shore`]: then
//! the fine mode decides per pixel, from the interpolated distance to the shoreline, whether
//! water, beach or land shows, so the shoreline is a smooth curve instead of following the
//! cells.

use crate::gfx::*;
use crate::math::*;

/// World height of a cell with the largest stored height.
pub const HMAX: f64 = 0.55;
/// Stored heights run from 0 to this value.
pub const HSCALE: f64 = 1000.0;
/// Farthest distance a ray travels (world units).
pub const ZMAX: f64 = 2.6;
/// Sky colour at the horizon; distant voxels fade into it.
pub const FOG: u32 = rgb(16, 34, 27);

/// Camera of one frame: position, viewing direction (horizontal) and projection.
pub struct Camera {
    /// Position, x.
    pub x: f64,
    /// Position, y.
    pub y: f64,
    /// Height above the ground plane.
    pub z: f64,
    /// Forward direction, x (unit length).
    pub fx: f64,
    /// Forward direction, y.
    pub fy: f64,
    /// Right direction, x.
    pub rx: f64,
    /// Right direction, y.
    pub ry: f64,
    /// Projection scale in pixels.
    pub focal: f64,
    /// Screen row of the horizon (smaller = looking further down).
    pub horizon: f64,
}

/// A path around the centre of the landscape.
pub struct Orbit {
    /// Angular speed in radians per second.
    pub speed: f64,
    /// Mean distance from the centre.
    pub radius: f64,
    /// How much the distance breathes in and out.
    pub breathe: f64,
    /// Angular rate of the breathing (radians per second).
    pub breathe_rate: f64,
    /// Mean height.
    pub height: f64,
    /// How much the height rises and sinks.
    pub rise: f64,
    /// Angular rate of rising and sinking (radians per second).
    pub rise_rate: f64,
    /// Phase of rising and sinking (radians).
    pub rise_phase: f64,
    /// Viewing direction relative to "towards the centre" in radians (positive: the
    /// centre lies to the left of the view).
    pub look: f64,
    /// Screen row of the horizon as a fraction of the screen height (used if `aim` is 0).
    pub horizon: f64,
    /// If > 0: the horizon follows distance and height so that the centre (at height
    /// `aim_height`) appears at this fraction of the screen height.
    pub aim: f64,
    /// Height of the point the camera aims at.
    pub aim_height: f64,
}

impl Orbit {
    /// The camera on this orbit.
    ///
    /// # Arguments
    /// * `t` - time in seconds
    /// * `dip` - how far the camera dips (e.g. a decaying beat pulse times a depth)
    /// * `h` - screen height in pixels
    ///
    /// # Returns
    /// The camera at time `t`.
    pub fn camera(&self, t: f64, dip: f64, h: i32) -> Camera {
        let ang = t * self.speed;
        let r = self.radius + self.breathe * sin(t * self.breathe_rate);
        let yaw = ang + PI + self.look;
        let (fx, fy) = (cos(yaw), sin(yaw));
        let z = self.height + self.rise * sin(t * self.rise_rate + self.rise_phase) - dip;
        let focal = h as f64 * 0.85;
        let horizon = if self.aim > 0.0 {
            h as f64 * self.aim - (z - self.aim_height) * focal / (r * cos(self.look))
        } else {
            h as f64 * self.horizon
        };
        Camera {
            x: r * cos(ang),
            y: r * sin(ang),
            z,
            fx,
            fy,
            rx: fy,
            ry: 0.0 - fx,
            focal,
            horizon,
        }
    }
}

/// Optional water layer of a height map (all pointers 0: none; see [`NO_SHORE`]). Every
/// layer has one value per cell of the map.
pub struct Shore {
    /// Signed distance of each cell to the shoreline in world units (negative: water).
    pub edge: *const f32,
    /// Heights of the water layer (cells on land: water at rest).
    pub water_heights: *const u16,
    /// Lit colours of the water layer.
    pub water_colours: *const u32,
    /// Heights of the land layer (cells in the water: the land continued below it).
    pub land_heights: *const u16,
    /// Lit colours of the land above the beach.
    pub land_colours: *const u32,
    /// Lit colours of the beach.
    pub beach_colours: *const u32,
    /// Width of the beach in world units.
    pub beach: f64,
    /// Width of the soft transitions (water to beach, beach to land) in world units.
    pub blend: f64,
}

/// No water layer.
pub const NO_SHORE: Shore = Shore {
    edge: 0 as *const f32,
    water_heights: 0 as *const u16,
    water_colours: 0 as *const u32,
    land_heights: 0 as *const u16,
    land_colours: 0 as *const u32,
    beach_colours: 0 as *const u32,
    beach: 0.0,
    blend: 1.0,
};

/// A height map with colours, placed in the world.
pub struct HeightMap {
    /// Heights, `w`·`h` values from 0 to [`HSCALE`].
    pub heights: *const u16,
    /// Lit colours, `w`·`h` values.
    pub colours: *const u32,
    /// Width in cells.
    pub w: i32,
    /// Height in cells.
    pub h: i32,
    /// Cell column of world x = 0.
    pub x0: f64,
    /// Cell row of world y = 0 (rows grow towards negative y).
    pub y0: f64,
    /// Cells per world unit.
    pub k: f64,
    /// The landscape ends at |x| or |y| > extent (world units).
    pub extent: f64,
    /// Optional water layer, used by the fine mode ([`NO_SHORE`] for none).
    pub shore: Shore,
}

impl HeightMap {
    /// Cell index of a world point.
    ///
    /// # Arguments
    /// * `wx` - world x
    /// * `wy` - world y
    ///
    /// # Returns
    /// The index into `heights`/`colours`, or -1 outside the landscape or the map.
    #[inline(always)]
    pub fn index(&self, wx: f64, wy: f64) -> i32 {
        let e = self.extent;
        if wx < 0.0 - e || wx > e || wy < 0.0 - e || wy > e {
            return -1;
        }
        let bx = self.x0 + wx * self.k;
        let by = self.y0 - wy * self.k;
        if bx < 0.0 || by < 0.0 {
            return -1;
        }
        let (x, y) = (bx as i32, by as i32);
        if x >= self.w || y >= self.h {
            return -1;
        }
        y * self.w + x
    }

    /// The four cells around a fractional cell position (cell centres at whole numbers)
    /// and the weights between them.
    ///
    /// # Arguments
    /// * `bx` - column, fractional
    /// * `by` - row, fractional
    ///
    /// # Returns
    /// The cell indices top left, top right, bottom left, bottom right and the fractions
    /// `(tx, ty)` towards the right and bottom cells.
    #[inline(always)]
    pub fn corners(&self, bx: f64, by: f64) -> ([usize; 4], f64, f64) {
        let fx = floor(bx);
        let fy = floor(by);
        let x0 = iclamp(fx as i32, 0, self.w - 1);
        let y0 = iclamp(fy as i32, 0, self.h - 1);
        let x1 = imin(x0 + 1, self.w - 1);
        let y1 = imin(y0 + 1, self.h - 1);
        let i = [
            (y0 * self.w + x0) as usize,
            (y0 * self.w + x1) as usize,
            (y1 * self.w + x0) as usize,
            (y1 * self.w + x1) as usize,
        ];
        (i, bx - fx, by - fy)
    }

    /// Bilinear height between four cells (see [`HeightMap::corners`]).
    ///
    /// # Safety
    /// `heights` must point to `w`·`h` readable values.
    #[inline(always)]
    pub unsafe fn height_between(&self, i: &[usize; 4], tx: f64, ty: f64) -> f64 {
        let top = self.height(i[0]) * (1.0 - tx) + self.height(i[1]) * tx;
        let bottom = self.height(i[2]) * (1.0 - tx) + self.height(i[3]) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// Bilinear colour between four cells (see [`HeightMap::corners`]).
    ///
    /// # Safety
    /// `colours` must point to `w`·`h` readable values.
    #[inline(always)]
    pub unsafe fn colour_between(&self, i: &[usize; 4], tx: f64, ty: f64) -> u32 {
        let (ux, uy) = ((tx * 256.0) as u32, (ty * 256.0) as u32);
        mix(
            mix(self.colour(i[0]), self.colour(i[1]), ux),
            mix(self.colour(i[2]), self.colour(i[3]), ux),
            uy,
        )
    }

    #[inline(always)]
    unsafe fn height(&self, i: usize) -> f64 {
        *((self.heights as usize + i * 2) as *const u16) as f64
    }

    #[inline(always)]
    unsafe fn colour(&self, i: usize) -> u32 {
        *((self.colours as usize + i * 4) as *const u32)
    }
}

/// Renders the landscape over what is already on the surface (e.g. a sky).
///
/// # Arguments
/// * `s` - target surface
/// * `cam` - camera
/// * `map` - the landscape
/// * `flash` - white flash, 0 (none) ..= 256
/// * `fine` - `false`: blocky voxels (nearest cell); `true`: smooth surface (bilinear,
///   finer ray steps, about twice the cost)
///
/// # Safety
/// `map.heights` and `map.colours` must point to `map.w`·`map.h` readable values.
pub unsafe fn render(s: &Surface, cam: &Camera, map: &HeightMap, flash: u32, fine: bool) {
    let mut col = 0;
    while col < s.w {
        if fine {
            column_fine(s, cam, map, col, flash);
        } else {
            column(s, cam, map, col, flash);
        }
        col += 1;
    }
}

/// Marches the ray of screen column `col` from near to far.
unsafe fn column(s: &Surface, cam: &Camera, map: &HeightMap, col: i32, flash: u32) {
    let off = (col as f64 - s.w as f64 / 2.0 + 0.5) / cam.focal;
    let dx = cam.fx + cam.rx * off;
    let dy = cam.fy + cam.ry * off;
    let mut ybuf = s.h;
    let mut z = 0.06;
    while z < ZMAX && ybuf > 0 {
        let idx = map.index(cam.x + dx * z, cam.y + dy * z);
        if idx >= 0 {
            let i = idx as usize;
            let hh = *((map.heights as usize + i * 2) as *const u16) as f64 * (HMAX / HSCALE);
            let sy = (cam.horizon + (cam.z - hh) * cam.focal / z) as i32;
            if sy < ybuf {
                let lit = *((map.colours as usize + i * 4) as *const u32);
                fill_span(s, col, imax(sy, 0), ybuf, shade(lit, z, flash));
                ybuf = sy;
            }
        }
        z += z * 0.011 + 0.0015;
    }
}

/// Like `column`, with bilinear heights and colours and finer steps.
unsafe fn column_fine(s: &Surface, cam: &Camera, map: &HeightMap, col: i32, flash: u32) {
    let off = (col as f64 - s.w as f64 / 2.0 + 0.5) / cam.focal;
    let dx = cam.fx + cam.rx * off;
    let dy = cam.fy + cam.ry * off;
    let mut ybuf = s.h;
    let mut z = 0.06;
    while z < ZMAX && ybuf > 0 {
        let wx = cam.x + dx * z;
        let wy = cam.y + dy * z;
        if map.index(wx, wy) >= 0 {
            let (i, tx, ty) = map.corners(map.x0 + wx * map.k - 0.5, map.y0 - wy * map.k - 0.5);
            let layered = map.shore.edge as usize != 0;
            let edge = if layered {
                lerp4(map.shore.edge as usize, 4, &i, tx, ty, true)
            } else {
                0.0
            };
            let hh = if !layered {
                map.height_between(&i, tx, ty)
            } else if edge < 0.0 {
                lerp4(map.shore.water_heights as usize, 2, &i, tx, ty, false)
            } else {
                lerp4(map.shore.land_heights as usize, 2, &i, tx, ty, false)
            };
            let sy = (cam.horizon + (cam.z - hh * (HMAX / HSCALE)) * cam.focal / z) as i32;
            if sy < ybuf {
                let lit = if layered {
                    shore_colour(&map.shore, edge, &i, tx, ty)
                } else {
                    map.colour_between(&i, tx, ty)
                };
                fill_span(s, col, imax(sy, 0), ybuf, shade(lit, z, flash));
                ybuf = sy;
            }
        }
        z += z * 0.0065 + 0.0009;
    }
}

/// Bilinear value between four cells of a layer: `u16` (size 2) or `f32` (size 4, `float`).
#[inline(always)]
unsafe fn lerp4(base: usize, size: usize, i: &[usize; 4], tx: f64, ty: f64, float: bool) -> f64 {
    let mut c = [0.0f64; 4];
    let mut k = 0;
    while k < 4 {
        let p = base + i[k] * size;
        c[k] = if float {
            *(p as *const f32) as f64
        } else {
            *(p as *const u16) as f64
        };
        k += 1;
    }
    (c[0] * (1.0 - tx) + c[1] * tx) * (1.0 - ty) + (c[2] * (1.0 - tx) + c[3] * tx) * ty
}

/// Bilinear colour between four cells of a colour layer.
#[inline(always)]
unsafe fn colour4(base: *const u32, i: &[usize; 4], tx: f64, ty: f64) -> u32 {
    let at = base as usize;
    let (ux, uy) = ((tx * 256.0) as u32, (ty * 256.0) as u32);
    let c0 = *((at + i[0] * 4) as *const u32);
    let c1 = *((at + i[1] * 4) as *const u32);
    let c2 = *((at + i[2] * 4) as *const u32);
    let c3 = *((at + i[3] * 4) as *const u32);
    mix(mix(c0, c1, ux), mix(c2, c3, ux), uy)
}

/// Colour at the shore for the interpolated edge distance: water, then the beach, then
/// land, with soft transitions of width `blend`.
#[inline(always)]
unsafe fn shore_colour(sh: &Shore, edge: f64, i: &[usize; 4], tx: f64, ty: f64) -> u32 {
    let to_land = clampf(0.5 + edge / sh.blend, 0.0, 1.0);
    let water = if to_land < 1.0 {
        colour4(sh.water_colours, i, tx, ty)
    } else {
        0
    };
    if to_land <= 0.0 {
        return water;
    }
    let to_grass = clampf(0.5 + (edge - sh.beach) / sh.blend, 0.0, 1.0);
    let beach = if to_grass < 1.0 {
        colour4(sh.beach_colours, i, tx, ty)
    } else {
        0
    };
    let land = if to_grass > 0.0 {
        colour4(sh.land_colours, i, tx, ty)
    } else {
        0
    };
    let ground = mix(beach, land, (to_grass * 256.0) as u32);
    mix(water, ground, (to_land * 256.0) as u32)
}

/// A lit colour faded into the fog with distance `z` and brightened by the flash.
#[inline(always)]
fn shade(lit: u32, z: f64, flash: u32) -> u32 {
    let fog = z / ZMAX;
    let c = mix(lit, FOG, (fog * fog * 230.0) as u32);
    if flash > 0 {
        mix(c, WHITE, flash)
    } else {
        c
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
