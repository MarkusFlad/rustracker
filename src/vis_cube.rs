//! Cube view: six live faces on a slowly tumbling cube with perspective-correct textures.
//!
//! Every frame the six faces are drawn into their own 256x256 textures (the same drawing
//! code as the big views), then the cube is projected and its visible faces are
//! rasterised as textured triangles with simple lighting.

use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::vis::*;

/// Corners of the six faces as vertex indices (tl, tr, br, bl seen from outside, y up);
/// vertex bits: bit 0 = x, bit 1 = y, bit 2 = z.
const FACES: [usize; 24] = [
    6, 7, 5, 4, 3, 2, 0, 1, 7, 3, 1, 5, 2, 6, 4, 0, 2, 3, 7, 6, 4, 5, 1, 0,
];
/// Which texture each face shows.
const FACE_TEX: [usize; 6] = [0, 2, 1, 3, 4, 5];
/// Camera distance in cube half-widths.
const DIST: f64 = 4.2;

/// A projected corner: screen position, 1/depth and texture coordinates.
struct Vertex {
    x: f64,
    y: f64,
    w: f64,
    u: f64,
    v: f64,
}

/// The eight rotated corners in 3D and on screen.
struct Projected {
    vx: [f64; 8],
    vy: [f64; 8],
    vz: [f64; 8],
    sx: [f64; 8],
    sy: [f64; 8],
    iw: [f64; 8],
}

impl Projected {
    /// Rotates the unit cube about x (`ax`) then y (`ay`) and projects it around
    /// (`cx`, `cy`) with the given focal length.
    fn new(ax: f64, ay: f64, focal: f64, cx: f64, cy: f64) -> Projected {
        let (sxa, cxa, sya, cya) = (sin(ax), cos(ax), sin(ay), cos(ay));
        let mut p = Projected {
            vx: [0.0; 8],
            vy: [0.0; 8],
            vz: [0.0; 8],
            sx: [0.0; 8],
            sy: [0.0; 8],
            iw: [0.0; 8],
        };
        let mut i = 0;
        while i < 8 {
            let x = if i & 1 != 0 { 1.0 } else { -1.0 };
            let yv = if i & 2 != 0 { 1.0 } else { -1.0 };
            let z = if i & 4 != 0 { 1.0 } else { -1.0 };
            let y1 = yv * cxa - z * sxa;
            let z1 = yv * sxa + z * cxa;
            let x2 = x * cya + z1 * sya;
            let z2 = 0.0 - x * sya + z1 * cya;
            p.vx[i] = x2;
            p.vy[i] = y1;
            p.vz[i] = z2;
            let w = 1.0 / (DIST - z2);
            p.iw[i] = w;
            p.sx[i] = cx + x2 * focal * w;
            p.sy[i] = cy - y1 * focal * w;
            i += 1;
        }
        p
    }

    fn vertex(&self, i: usize, u: f64, v: f64) -> Vertex {
        Vertex {
            x: self.sx[i],
            y: self.sy[i],
            w: self.iw[i],
            u,
            v,
        }
    }

    /// True if the face with corners a, b, d faces the camera.
    fn visible(&self, a: usize, b: usize, d: usize) -> bool {
        let (sx, sy) = (&self.sx, &self.sy);
        (sx[b] - sx[a]) * (sy[d] - sy[a]) - (sy[b] - sy[a]) * (sx[d] - sx[a]) > 0.0
    }

    /// Brightness (0..256) of the face from its normal and a fixed light direction.
    fn shade(&self, a: usize, b: usize, d: usize) -> u32 {
        let ux = self.vx[b] - self.vx[a];
        let uy = self.vy[b] - self.vy[a];
        let uz = self.vz[b] - self.vz[a];
        let wx = self.vx[d] - self.vx[a];
        let wy = self.vy[d] - self.vy[a];
        let wz = self.vz[d] - self.vz[a];
        let nx = uy * wz - uz * wy;
        let ny = uz * wx - ux * wz;
        let nz = ux * wy - uy * wx;
        let nl = sqrt(nx * nx + ny * ny + nz * nz);
        let lit = 0.45 + 0.55 * maxf(0.0, (0.0 - nx * 0.3 + ny * 0.5 + nz * 0.81) / nl);
        (lit * 256.0) as u32
    }
}

impl Vis {
    /// The texture of face `i` as a surface.
    pub(crate) fn face_surface(&mut self, i: usize) -> Surface {
        Surface {
            p: ((&raw mut self.face) as usize + i * FACE_N * FACE_N * 4) as *mut u32,
            w: FACE_N as i32,
            h: FACE_N as i32,
            stride: FACE_N as i32,
        }
    }

    /// The face title in the top left corner.
    pub(crate) fn face_label<const N: usize>(&self, s: &Surface, label: &[u8; N]) {
        s.fill_a(3, 3, N as i32 * 12 + 6, 18, BLACK, 160);
        s.text(6, 5, label, 2, PHOSPHOR);
    }

    /// Draws all six face textures.
    pub(crate) fn draw_faces(&mut self, now: f64, dt: f64, cur: i32, pl: &Player) {
        self.face_master(cur);
        self.face_xy(cur);
        let f = self.face_surface(2);
        f.clear(BG);
        f.grid(0, 0, f.w, f.h);
        self.draw_spec2(&f, dt, cur, false);
        self.face_label(&f, b"SPECTRUM");
        let f = self.face_surface(3);
        f.clear(BG);
        self.draw_spec3(&f, now, cur, false);
        self.face_label(&f, b"3D");
        self.face_notes(cur, pl);
        self.face_mandel();
    }

    fn face_master(&mut self, cur: i32) {
        let f = self.face_surface(0);
        f.clear(BG);
        f.grid(0, 0, f.w, f.h);
        if cur >= 0 {
            let k = cur as usize;
            let ml = self.msgs[k].nch as usize * SCOPE_LEN;
            let st = self.trigger(k, ml, 600) - ml;
            self.trace(&f, k, ml + SCOPE_LEN + st, 600, AMBER, 1.6);
            self.trace(&f, k, ml + st, 600, PHOSPHOR, 1.6);
        }
        self.face_label(&f, b"MASTER L/R");
    }

    fn face_xy(&mut self, cur: i32) {
        let f = self.face_surface(1);
        f.clear(BG);
        f.grid(0, 0, f.w, f.h);
        if cur >= 0 {
            self.draw_xy_figure(&f, cur as usize, 1.7, 190);
        }
        self.face_label(&f, b"XY");
    }

    /// The rows around the current one, notes only, at most four channels.
    fn face_notes(&mut self, cur: i32, pl: &Player) {
        let f = self.face_surface(4);
        f.clear(BG);
        if cur >= 0 {
            let m = &self.msgs[cur as usize];
            let (pat, row) = (m.pattern as usize, m.row as i32);
            let span = 12;
            let mut line = 0;
            while line <= 2 * span {
                let r = row - span + line;
                if r >= 0 && r < 64 {
                    face_note_row(&f, pl, pat, r, r == row, 24 + line * 9);
                }
                line += 1;
            }
        }
        self.face_label(&f, b"NOTES");
    }

    /// The finished Mandelbrot buffer, zoomed in a little and without rotation.
    fn face_mandel(&mut self) {
        let f = self.face_surface(5);
        f.clear(BLACK);
        let sz = self.v_size;
        self.v_size = sz * 0.75;
        let fl = self.d_flash;
        self.mb_blit(&f, 0.0, fl);
        self.v_size = sz;
        self.face_label(&f, b"MANDELBROT");
    }

    /// The cube view: renders the faces, then projects and rasterises the cube. It
    /// tumbles about two axes so that every face comes to the front, and pulses on beats.
    pub(crate) fn draw_cube(&mut self, s: &Surface, now: f64, dt: f64, cur: i32, pl: &Player) {
        self.keep_face_mandel_going();
        self.draw_faces(now, dt, cur, pl);
        draw_vertical_gradient(s, rgb(11, 23, 18), BG);
        self.pulse *= exp_neg(dt * 6.0);
        let t = (now - self.t0) / 1000.0;
        let ax = (-20.0 + t * 9.0) * PI / 180.0;
        let ay = (t * 14.0) * PI / 180.0;
        let size = imin(s.w, s.h) as f64 * 0.30 * (1.0 + 0.035 * self.pulse);
        let p = Projected::new(ax, ay, size * DIST, s.w as f64 / 2.0, s.h as f64 / 2.0);
        let mut f = 0;
        while f < 6 {
            self.draw_cube_face(s, &p, f);
            f += 1;
        }
    }

    /// The face Mandelbrot needs a square buffer of its own resolution.
    pub(crate) fn keep_face_mandel_going(&mut self) {
        if self.b_active && !self.b_face {
            self.b_active = false;
        }
        if !self.b_active {
            self.mb_start(FACE_N as i32, FACE_N as i32, true);
        }
        // DOS: a smaller share of Mandelbrot iterations per frame, so the cube keeps turning
        #[cfg(dos)]
        self.mb_work(300_000);
        #[cfg(not(dos))]
        self.mb_work(1_500_000);
    }

    /// Face `f` as two textured triangles plus its glowing edges, if it is visible.
    fn draw_cube_face(&self, s: &Surface, p: &Projected, f: usize) {
        let (a, b, c, d) = (
            FACES[f * 4],
            FACES[f * 4 + 1],
            FACES[f * 4 + 2],
            FACES[f * 4 + 3],
        );
        if !p.visible(a, b, d) {
            return;
        }
        let shade = p.shade(a, b, d);
        let tex = FACE_TEX[f];
        self.tri(
            s,
            &p.vertex(a, 0.0, 0.0),
            &p.vertex(b, 1.0, 0.0),
            &p.vertex(c, 1.0, 1.0),
            tex,
            shade,
        );
        self.tri(
            s,
            &p.vertex(a, 0.0, 0.0),
            &p.vertex(c, 1.0, 1.0),
            &p.vertex(d, 0.0, 1.0),
            tex,
            shade,
        );
        let corners: [usize; 4] = [a, b, c, d];
        let mut e = 0;
        while e < 4 {
            let (i, j) = (corners[e], corners[(e + 1) % 4]);
            s.line(
                p.sx[i] as f32,
                p.sy[i] as f32,
                p.sx[j] as f32,
                p.sy[j] as f32,
                PHOSPHOR,
                150,
            );
            e += 1;
        }
    }

    /// Perspective-correct textured triangle: texture `tex`, brightness `shade` (0..256).
    fn tri(&self, s: &Surface, a: &Vertex, b: &Vertex, c: &Vertex, tex: usize, shade: u32) {
        let minx = maxf(0.0, floor(minf(a.x, minf(b.x, c.x)))) as i32;
        let maxx = minf((s.w - 1) as f64, floor(maxf(a.x, maxf(b.x, c.x))) + 1.0) as i32;
        let miny = maxf(0.0, floor(minf(a.y, minf(b.y, c.y)))) as i32;
        let maxy = minf((s.h - 1) as f64, floor(maxf(a.y, maxf(b.y, c.y))) + 1.0) as i32;
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if absf(area) < 1e-6 {
            return;
        }
        let inv = 1.0 / area;
        let mut y = miny;
        while y <= maxy {
            let py = y as f64 + 0.5;
            let mut x = minx;
            while x <= maxx {
                let px = x as f64 + 0.5;
                let l0 = ((b.x - px) * (c.y - py) - (b.y - py) * (c.x - px)) * inv;
                let l1 = ((c.x - px) * (a.y - py) - (c.y - py) * (a.x - px)) * inv;
                let l2 = 1.0 - l0 - l1;
                if l0 >= 0.0 && l1 >= 0.0 && l2 >= 0.0 {
                    s.set(x, y, dim(self.texel(tex, a, b, c, l0, l1, l2), shade));
                }
                x += 1;
            }
            y += 1;
        }
    }

    /// Texture colour at barycentric coordinates (l0, l1, l2), perspective-corrected.
    #[inline(always)]
    fn texel(
        &self,
        tex: usize,
        a: &Vertex,
        b: &Vertex,
        c: &Vertex,
        l0: f64,
        l1: f64,
        l2: f64,
    ) -> u32 {
        let nmax = (FACE_N - 1) as f64;
        let ww = l0 * a.w + l1 * b.w + l2 * c.w;
        let u = (l0 * a.u * a.w + l1 * b.u * b.w + l2 * c.u * c.w) / ww;
        let v = (l0 * a.v * a.w + l1 * b.v * b.w + l2 * c.v * c.w) / ww;
        let tx = clampf(u * nmax, 0.0, nmax) as usize;
        let ty = clampf(v * nmax, 0.0, nmax) as usize;
        let tbase = (&raw const self.face) as usize + tex * FACE_N * FACE_N * 4;
        unsafe { *((tbase + (ty * FACE_N + tx) * 4) as *const u32) }
    }
}

/// One row of the notes face: row number and the notes of up to four channels.
fn face_note_row(f: &Surface, pl: &Player, pat: usize, r: i32, current: bool, y: i32) {
    if current {
        f.fill(0, y - 1, f.w, 9, PHOSPHOR);
    }
    let col = if current {
        BLACK
    } else if r % 4 == 0 {
        TEXT
    } else {
        MUTED
    };
    let nch = if pl.channels > 4 { 4 } else { pl.channels };
    let mut t = TextBuf::new();
    t.num(r as u32, 2, 48);
    let mut c = 0;
    while c < nch {
        t.ch(32);
        t.note(pl.cell(pat, r as usize, c).period);
        c += 1;
    }
    f.text_buf(4, y, &t, 1, col);
}

/// Fills the surface with a vertical gradient from `top` to `bottom`.
pub(crate) fn draw_vertical_gradient(s: &Surface, top: u32, bottom: u32) {
    let mut y = 0;
    while y < s.h {
        let t = (y * 256 / s.h) as u32;
        s.hline(0, s.w - 1, y, mix(top, bottom, t), 256);
        y += 1;
    }
}
