//! Demo-scene view: a bow to the demo scene, where trackers and the MOD format grew up.
//!
//! Every position of the song switches to the next classic effect (order position modulo 4):
//!
//! 0. **Amiga**: copper bars, one per channel, swinging through a starfield whose speed
//!    follows the music level; a bar flashes when its channel plays a note,
//! 1. **PC**: a plasma of overlapping sine waves with palette cycling,
//! 2. **PC**: a rotozoomer, a rotating and zooming texture that pulses on the beat,
//! 3. **C64**: raster bars in the border around the blue screen, one per channel.
//!
//! Over all parts: a logo in cycling colours that hops on every beat and a sine scroller
//! with the song title, the sample names (where musicians traditionally hid greetings) and
//! greetings to the scene. The inner loops of plasma and rotozoomer use only integer
//! arithmetic and tables, so they stay fast on the DOS edition too.

use crate::font;
use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::vis::*;

/// Stars in the starfield.
pub(crate) const DM_STARS: usize = 220;
/// Capacity of the scroll text.
pub(crate) const DM_TEXT: usize = 1024;
/// Copper bar colours, one per channel.
const BAR_COLOURS: [u32; 8] = [
    rgb(255, 60, 60),
    rgb(255, 160, 40),
    rgb(250, 230, 60),
    rgb(57, 255, 136),
    rgb(40, 220, 255),
    rgb(70, 110, 255),
    rgb(180, 80, 255),
    rgb(255, 70, 200),
];
/// Colours of the seven font rows of the scroller, top to bottom.
const SCROLL_ROWS: [u32; 7] = [
    rgb(255, 255, 210),
    rgb(255, 240, 120),
    rgb(255, 200, 60),
    rgb(255, 150, 40),
    rgb(255, 100, 40),
    rgb(230, 60, 60),
    rgb(170, 40, 90),
];

impl Vis {
    /// The demo-scene view: the part of the current song position, logo and sine scroller.
    pub(crate) fn draw_demo(&mut self, s: &Surface, now: f64, dt: f64, cur: i32, pl: &Player) {
        if !self.dm_ready {
            self.build_scroll_text(pl);
            self.init_stars();
            self.build_tables();
            self.dm_ready = true;
        }
        if cur >= 0 && self.playing {
            self.note_flashes(cur as usize, pl);
        }
        self.decay_flashes(dt);
        let t = (now - self.t0) / 1000.0;
        let part = if cur >= 0 {
            self.msgs[cur as usize].order % 4
        } else {
            0
        };
        if part == 1 {
            self.draw_plasma(s, t);
            caption(s, b"PC: PLASMA");
        } else if part == 2 {
            self.draw_rotozoomer(s, t);
            caption(s, b"PC: ROTOZOOMER");
        } else if part == 3 {
            self.draw_c64(s, t, cur, pl);
            caption(s, b"C64: RASTER BARS");
        } else {
            s.clear(BLACK);
            self.draw_stars(s, dt);
            self.draw_copper_bars(s, t, cur, pl);
            caption(s, b"AMIGA: COPPER-BARS");
        }
        self.draw_logo(s, t);
        self.draw_scroller(s, t, dt);
    }

    /// Scroll text: title, sample names and a few words about the player.
    fn build_scroll_text(&mut self, pl: &Player) {
        self.dm_len = 0;
        self.scroll_str(b"RUSTRACKER GREETS THE DEMO SCENE AND PLAYS:   ");
        let mut i = 0;
        while i < 20 {
            self.scroll_byte(pl.mod_byte(i));
            i += 1;
        }
        self.scroll_str(b"   ***   ");
        let mut sm = 0;
        while sm < pl.num_samples() {
            self.scroll_sample_name(pl, sm);
            sm += 1;
        }
        self.scroll_str(b"   ***   GREETINGS TO ALL CODERS, GRAPHICIANS AND MUSICIANS  -  ON C64, AMIGA, ATARI ST AND PC  -  TO THE INVENTORS OF THE TRACKER  -  TO EVERYONE CODING THROUGH THE NIGHT AT A PARTY  -  A MOD PLAYER IN RUST, EVERY PIXEL HANDMADE   ***          ");
    }

    /// Appends the name of sample `sm` (trimmed, unprintable bytes as spaces) and a separator.
    fn scroll_sample_name(&mut self, pl: &Player, sm: usize) {
        let base = 20 + sm * 30;
        let mut end = 22;
        while end > 0 && printable(pl.mod_byte(base + end - 1)) == 32 {
            end -= 1;
        }
        let mut start = 0;
        while start < end && printable(pl.mod_byte(base + start)) == 32 {
            start += 1;
        }
        if start == end {
            return;
        }
        let mut i = start;
        while i < end {
            self.scroll_byte(pl.mod_byte(base + i));
            i += 1;
        }
        self.scroll_str(b"   ");
    }

    fn scroll_str<const N: usize>(&mut self, s: &[u8; N]) {
        let mut i = 0;
        while i < N {
            self.scroll_byte(s[i] as u32);
            i += 1;
        }
    }

    fn scroll_byte(&mut self, b: u32) {
        if self.dm_len < DM_TEXT {
            self.dm_text[self.dm_len] = printable(b) as u8;
            self.dm_len += 1;
        }
    }

    /// A new row of the current slot's pattern: channels with a note flash; beats make
    /// the logo hop.
    fn note_flashes(&mut self, k: usize, pl: &Player) {
        let m = self.msgs[k].copy();
        let key = m.order * 64 + m.row;
        if key == self.dm_rowkey {
            return;
        }
        self.dm_rowkey = key;
        let mut c = 0;
        while c < 8 && c < pl.channels {
            if pl.cell(m.pattern as usize, m.row as usize, c).period != 0 {
                self.dm_flash[c] = 1.0;
            }
            c += 1;
        }
        if m.row % 4 == 0 {
            self.dm_pulse = if m.row % 16 == 0 { 1.0 } else { 0.6 };
        }
    }

    fn decay_flashes(&mut self, dt: f64) {
        let k = exp_neg(dt * 5.0) as f32;
        let mut c = 0;
        while c < 8 {
            self.dm_flash[c] *= k;
            c += 1;
        }
        self.dm_pulse *= exp_neg(dt * 7.0);
    }

    // ---------------------------------------------------------------- starfield

    fn init_stars(&mut self) {
        self.dm_seed = 0x1234_5678;
        let mut i = 0;
        while i < DM_STARS {
            let z = self.random() as f32;
            self.new_star(i, z);
            i += 1;
        }
    }

    /// Star `i` at a random direction and depth `z` (0 = at the camera, 1 = far).
    fn new_star(&mut self, i: usize, z: f32) {
        self.dm_stars[i * 3] = (self.random() * 2.0 - 1.0) as f32;
        self.dm_stars[i * 3 + 1] = (self.random() * 2.0 - 1.0) as f32;
        self.dm_stars[i * 3 + 2] = z;
    }

    /// Next pseudo-random number in 0..1 (linear congruential generator).
    fn random(&mut self) -> f64 {
        self.dm_seed = ((self.dm_seed as u64 * 1_664_525 + 1_013_904_223) & 0xffff_ffff) as u32;
        (self.dm_seed >> 8) as f64 / 16_777_216.0
    }

    /// Stars fly towards the camera, faster when the music is loud.
    fn draw_stars(&mut self, s: &Surface, dt: f64) {
        let speed = (0.15 + 0.6 * self.inten) * dt;
        let (cx, cy) = (s.w as f32 / 2.0, s.h as f32 / 2.0);
        let mut i = 0;
        while i < DM_STARS {
            let z = self.dm_stars[i * 3 + 2] - speed as f32;
            if z <= 0.03 {
                self.new_star(i, 1.0);
                i += 1;
                continue;
            }
            self.dm_stars[i * 3 + 2] = z;
            let x = cx + self.dm_stars[i * 3] / z * cx;
            let y = cy + self.dm_stars[i * 3 + 1] / z * cx;
            let b = (60.0 + (1.0 - z) * 196.0) as u32;
            let size = if z < 0.3 { imax(self.fs, 2) } else { 1 };
            s.fill_a(x as i32, y as i32, size, size, rgb(200, 220, 255), b + 1);
            i += 1;
        }
    }

    // ---------------------------------------------------------------- copper bars

    /// One horizontal bar per channel, swinging on sine paths; drawn back to front.
    fn draw_copper_bars(&self, s: &Surface, t: f64, cur: i32, pl: &Player) {
        let nch = if cur >= 0 {
            self.msgs[cur as usize].nch as usize
        } else {
            pl.channels
        };
        let n = umin(imax(nch as i32, 1) as usize, 8);
        // order by depth: z = cos(phase), smaller z = further back
        let mut order = [0usize; 8];
        let mut i = 0;
        while i < n {
            order[i] = i;
            i += 1;
        }
        let mut a = 1;
        while a < n {
            let mut b = a;
            while b > 0 && bar_depth(t, order[b]) < bar_depth(t, order[b - 1]) {
                let tmp = order[b];
                order[b] = order[b - 1];
                order[b - 1] = tmp;
                b -= 1;
            }
            a += 1;
        }
        i = 0;
        while i < n {
            self.draw_copper_bar(s, t, order[i]);
            i += 1;
        }
    }

    /// Copper bar of channel `c`: a vertical gradient dark–colour–white–colour–dark.
    fn draw_copper_bar(&self, s: &Surface, t: f64, c: usize) {
        let ph = t * 1.3 + c as f64 * 0.78;
        let z = cos(ph);
        let thick = (s.h as f64 * 0.07 * (0.7 + 0.3 * z)) as i32 + 2;
        let y0 = (s.h as f64 * (0.47 + 0.2 * sin(ph))) as i32 - thick / 2;
        let flash = self.dm_flash[c] as f64;
        let colour = dim(BAR_COLOURS[c], (150.0 + 106.0 * (0.5 + 0.5 * z)) as u32);
        let mut j = 0;
        while j < thick {
            let p = sin(PI * (j as f64 + 0.5) / thick as f64);
            let mut col = mix(BLACK, colour, (p * 256.0) as u32);
            let white = p * p * p * p * (0.25 + 0.75 * flash);
            col = mix(col, WHITE, (white * 256.0) as u32);
            s.hline(0, s.w - 1, y0 + j, col, 256);
            j += 1;
        }
    }

    // ---------------------------------------------------------------- tables

    /// Sine table (-128..127 for one turn in 256 steps), the plasma palette and the
    /// rotozoomer texture with its palette.
    fn build_tables(&mut self) {
        let mut i = 0;
        while i < 256 {
            let a = i as f64 * 2.0 * PI / 256.0;
            self.dm_sin[i] = (sin(a) * 127.0) as i32;
            let r = 128.0 + 127.0 * sin(a);
            let g = 128.0 + 127.0 * sin(a + 2.0 * PI / 3.0);
            let b = 128.0 + 127.0 * sin(a + 4.0 * PI / 3.0);
            self.dm_plasma_pal[i] = rgb(r as u32, g as u32, b as u32);
            let k = i as f64 / 255.0;
            self.dm_tex_pal[i] = rgb(
                (40.0 + 215.0 * k * k) as u32,
                (30.0 + 160.0 * k) as u32,
                (90.0 + 165.0 * sqrt(k)) as u32,
            );
            i += 1;
        }
        // XOR texture, darkened in a checkerboard of 32 x 32 texels
        let mut y = 0;
        while y < 256 {
            let mut x = 0;
            while x < 256 {
                let v = (x ^ y) as u32;
                let dark = ((x >> 5) + (y >> 5)) & 1 == 1;
                self.dm_tex[y * 256 + x] = if dark { (v / 2) as u8 } else { v as u8 };
                x += 1;
            }
            y += 1;
        }
    }

    // ---------------------------------------------------------------- PC: plasma

    /// Four sine waves over x, y, x + y and a bent y; the sum indexes a cycling palette.
    fn draw_plasma(&self, s: &Surface, t: f64) {
        let (w, h) = (umin(s.w as usize, 2048), s.h);
        let tf = (t * 256.0) as i32;
        let (t1, t2, t3, t4) = (tf / 3, tf / 5, tf / 2, tf / 7);
        let shift = (t * 70.0 + self.dm_pulse * 40.0) as i32;
        // per column: texture coordinate (256 per screen width) and the x wave
        let mut ux = [0i32; 2048];
        let mut wx = [0i32; 2048];
        let mut x = 0;
        while x < w {
            ux[x] = (x * 256 / w) as i32;
            wx[x] = self.dm_sin[((ux[x] * 2 + t1) & 255) as usize];
            x += 1;
        }
        let mut y = 0;
        while y < h {
            let vy = y * 256 / w as i32;
            let wy = self.dm_sin[((vy * 2 + t2) & 255) as usize];
            let bend = self.dm_sin[((vy + t4) & 255) as usize] / 2;
            let row = s.p as usize + (y * s.stride) as usize * 4;
            x = 0;
            while x < w {
                let u = ux[x];
                let v = wx[x]
                    + wy
                    + self.dm_sin[((u + vy + t3) & 255) as usize]
                    + self.dm_sin[((u * 3 + bend + tf / 4) & 255) as usize];
                let c = self.dm_plasma_pal[(((v >> 2) + shift) & 255) as usize];
                unsafe { *((row + x * 4) as *mut u32) = c };
                x += 1;
            }
            y += 1;
        }
    }

    // ---------------------------------------------------------------- PC: rotozoomer

    /// The XOR texture, rotated and zoomed around the screen centre (16.16 fixed point).
    fn draw_rotozoomer(&self, s: &Surface, t: f64) {
        let ang = t * 0.45;
        let zoom = (1.3 + 0.7 * sin(t * 0.53)) * (1.0 - 0.3 * self.dm_pulse);
        let scale = zoom * 256.0 / s.w as f64 * 65536.0; // texels per pixel
        let du = (cos(ang) * scale) as i32;
        let dv = (sin(ang) * scale) as i32;
        let (hw, hh) = (s.w / 2, s.h / 2);
        let drift = ((t * 40.0 * 65536.0) as i64 & 0xff_ffff) as i32; // 256 texels wrap
        let u0 = (128 << 16) + drift - hw * du + hh * dv;
        let v0 = (128 << 16) - hw * dv - hh * du;
        let mut y = 0;
        while y < s.h {
            let mut u = u0 - y * dv;
            let mut v = v0 + y * du;
            let row = s.p as usize + (y * s.stride) as usize * 4;
            let mut x = 0;
            while x < s.w as usize {
                let texel = ((v >> 8) & 0xff00) + ((u >> 16) & 0xff);
                let c = self.dm_tex_pal[self.dm_tex[texel as usize] as usize];
                unsafe { *((row + x * 4) as *mut u32) = c };
                u += du;
                v += dv;
                x += 1;
            }
            y += 1;
        }
    }

    // ---------------------------------------------------------------- C64: raster bars

    /// The blue screen with "READY." and a blinking cursor; raster bars, one per channel,
    /// run through the border and flash with the notes.
    fn draw_c64(&self, s: &Surface, t: f64, cur: i32, pl: &Player) {
        let nch = if cur >= 0 {
            self.msgs[cur as usize].nch as usize
        } else {
            pl.channels
        };
        let n = umin(imax(nch as i32, 1) as usize, 8);
        s.clear(C64[14]); // light blue border
        let mut c = 0;
        while c < n {
            let flash = self.dm_flash[c] as f64;
            let band = imax(s.h / 70, 1) + (flash * (s.h / 60) as f64) as i32;
            let yc = (s.h as f64 * (0.5 + 0.4 * sin(t * 1.6 + c as f64 * 0.8))) as i32;
            let cols = &RASTER[c % 4];
            let mut k = 0;
            while k < 7 {
                s.fill(
                    0,
                    yc + (k - 3) * band - band / 2,
                    s.w,
                    band,
                    C64[cols[k as usize]],
                );
                k += 1;
            }
            c += 1;
        }
        let (bx, by) = (s.w / 10, s.h / 7);
        s.fill(bx, by, s.w - 2 * bx, s.h - 2 * by, C64[6]); // blue screen
        let scale = imax(s.h / 160, 1);
        let lh = 9 * scale;
        let x = bx + 4 * scale;
        s.text(x, by + 4 * scale + 2 * lh, b"READY.", scale, C64[14]);
        if (t * 2.0) as u32 & 1 == 0 {
            s.fill(
                x,
                by + 4 * scale + 3 * lh - scale,
                6 * scale,
                8 * scale,
                C64[14],
            );
        }
    }

    // ---------------------------------------------------------------- logo and scroller

    /// "RUSTRACKER" at the top, rows in cycling copper colours, hopping on beats.
    fn draw_logo(&self, s: &Surface, t: f64) {
        let dot = imax(s.w / 110, 2);
        let x0 = (s.w - 10 * 6 * dot) / 2;
        let y0 = (s.h as f64 * 0.06 - self.dm_pulse * dot as f64 * 1.5) as i32;
        let shift = (t * 6.0) as usize;
        let mut rows = [0u32; 7];
        let mut r = 0;
        while r < 7 {
            rows[r] = BAR_COLOURS[(r + shift) % 8];
            r += 1;
        }
        let text = b"RUSTRACKER";
        let mut i = 0;
        while i < 10 {
            let mut col = 0;
            while col < 5 {
                let x = x0 + (i as i32 * 6 + col) * dot;
                draw_glyph_column(
                    s,
                    text[i] as u32,
                    col,
                    x + dot / 2,
                    y0 + dot / 2,
                    dot,
                    &[BLACK; 7],
                    180,
                );
                draw_glyph_column(s, text[i] as u32, col, x, y0, dot, &rows, 256);
                col += 1;
            }
            i += 1;
        }
    }

    /// The sine scroller near the bottom: every dot column follows a travelling sine wave.
    fn draw_scroller(&mut self, s: &Surface, t: f64, dt: f64) {
        if self.dm_len == 0 {
            return;
        }
        self.dm_scroll += dt * 4.5; // characters per second
        if self.dm_scroll >= self.dm_len as f64 {
            self.dm_scroll -= self.dm_len as f64;
        }
        let dot = imax(s.h / 52, 2);
        let cw = 6 * dot;
        let base_y = (s.h as f64 * 0.8) as i32 - 7 * dot / 2;
        let amp = s.h as f64 * 0.045;
        let first = self.dm_scroll as usize;
        let frac = self.dm_scroll - first as f64;
        let visible = s.w / cw + 2;
        let mut k = 0;
        while k < visible {
            let ch = self.dm_text[(first + k as usize) % self.dm_len] as u32;
            let x = ((k as f64 - frac) * cw as f64) as i32;
            let mut col = 0;
            while col < 5 {
                let px = x + col * dot;
                let y = base_y + (amp * sin(px as f64 / s.w as f64 * 1.5 * PI + t * 2.6)) as i32;
                draw_glyph_column(s, ch, col, px + dot / 2, y + dot / 2, dot, &[BLACK; 7], 200);
                draw_glyph_column(s, ch, col, px, y, dot, &SCROLL_ROWS, 256);
                col += 1;
            }
            k += 1;
        }
    }
}

/// Draws column `col` (0..4) of a character of the built-in font as square dots of size
/// `dot`, row `r` in `colours[r]`, with opacity `alpha` (256 = opaque).
#[allow(clippy::too_many_arguments)]
fn draw_glyph_column(
    s: &Surface,
    ch: u32,
    col: i32,
    x: i32,
    y: i32,
    dot: i32,
    colours: &[u32; 7],
    alpha: u32,
) {
    if x + dot <= 0 || x >= s.w {
        return;
    }
    let base = ((glyph_code(ch) - font::FIRST) * 7) as usize;
    let mut r = 0;
    while r < 7 {
        let bits = font::GLYPHS[base + r] as u32;
        if (bits >> (4 - col) as u32) & 1 != 0 {
            let yy = y + r as i32 * dot;
            if alpha >= 256 {
                s.fill(x, yy, dot, dot, colours[r]);
            } else {
                s.fill_a(x, yy, dot, dot, colours[r], alpha);
            }
        }
        r += 1;
    }
}

/// The name of the current part in the bottom left corner, on a dark shadow.
fn caption<const N: usize>(s: &Surface, text: &[u8; N]) {
    let scale = imax(s.h / 200, 1);
    let y = s.h - 9 * scale;
    s.text(4 + scale, y + scale, text, scale, BLACK);
    s.text(4, y, text, scale, WHITE);
}

/// Depth of channel `c`'s bar at time `t` (-1 back .. 1 front).
fn bar_depth(t: f64, c: usize) -> f64 {
    cos(t * 1.3 + c as f64 * 0.78)
}

/// A byte as a printable character (others as a space).
fn printable(b: u32) -> u32 {
    if b >= 32 && b < 127 {
        b
    } else {
        32
    }
}

/// The 16 colours of the C64 (after the usual measurements of the VIC-II output).
const C64: [u32; 16] = [
    rgb(0, 0, 0),
    rgb(255, 255, 255),
    rgb(136, 57, 50),
    rgb(103, 182, 189),
    rgb(139, 63, 150),
    rgb(85, 160, 73),
    rgb(64, 49, 141),
    rgb(191, 206, 114),
    rgb(139, 84, 41),
    rgb(87, 66, 0),
    rgb(184, 105, 98),
    rgb(80, 80, 80),
    rgb(120, 120, 120),
    rgb(148, 224, 137),
    rgb(120, 105, 196),
    rgb(159, 159, 159),
];
/// Raster bars as seven C64 colours from dark to bright to dark: red, green, blue, grey.
const RASTER: [[usize; 7]; 4] = [
    [9, 2, 10, 1, 10, 2, 9],
    [11, 5, 13, 1, 13, 5, 11],
    [6, 4, 14, 1, 14, 4, 6],
    [11, 12, 15, 1, 15, 12, 11],
];
