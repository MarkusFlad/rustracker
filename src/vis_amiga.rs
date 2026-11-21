//! Amiga view: a homage to the demos the MOD format comes from.
//!
//! * a starfield whose speed follows the music level,
//! * one copper bar per channel, swinging through the picture and flashing when its
//!   channel plays a note (bars further back are drawn first and thinner),
//! * a logo in copper colours that hops on every beat,
//! * a sine scroller in the built-in font with the song title and the sample names, where
//!   musicians traditionally hid greetings and messages.

use crate::font;
use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::vis::*;

/// Stars in the starfield.
pub(crate) const AM_STARS: usize = 220;
/// Capacity of the scroll text.
pub(crate) const AM_TEXT: usize = 1024;
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
    /// The Amiga view: starfield, copper bars, logo and sine scroller.
    pub(crate) fn draw_amiga(&mut self, s: &Surface, now: f64, dt: f64, cur: i32, pl: &Player) {
        if !self.am_ready {
            self.build_scroll_text(pl);
            self.init_stars();
            self.am_ready = true;
        }
        if cur >= 0 && self.playing {
            self.note_flashes(cur as usize, pl);
        }
        self.decay_flashes(dt);
        let t = (now - self.t0) / 1000.0;
        s.clear(BLACK);
        self.draw_stars(s, dt);
        self.draw_copper_bars(s, t, cur, pl);
        self.draw_logo(s, t);
        self.draw_scroller(s, t, dt);
    }

    /// Scroll text: title, sample names and a few words about the player.
    fn build_scroll_text(&mut self, pl: &Player) {
        self.am_len = 0;
        self.scroll_str(b"RUSTRACKER ZEIGT:   ");
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
        self.scroll_str(b"   ***   EIN MOD-PLAYER IN RUST UND WEBASSEMBLY  -  JEDES PIXEL HANDGEMACHT  -  HALLO AN ALLE TRACKER-FANS!   ***          ");
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
        if self.am_len < AM_TEXT {
            self.am_text[self.am_len] = printable(b) as u8;
            self.am_len += 1;
        }
    }

    /// A new row of the current slot's pattern: channels with a note flash; beats make
    /// the logo hop.
    fn note_flashes(&mut self, k: usize, pl: &Player) {
        let m = self.msgs[k].copy();
        let key = m.order * 64 + m.row;
        if key == self.am_rowkey {
            return;
        }
        self.am_rowkey = key;
        let mut c = 0;
        while c < 8 && c < pl.channels {
            if pl.cell(m.pattern as usize, m.row as usize, c).period != 0 {
                self.am_flash[c] = 1.0;
            }
            c += 1;
        }
        if m.row % 4 == 0 {
            self.am_pulse = if m.row % 16 == 0 { 1.0 } else { 0.6 };
        }
    }

    fn decay_flashes(&mut self, dt: f64) {
        let k = exp_neg(dt * 5.0) as f32;
        let mut c = 0;
        while c < 8 {
            self.am_flash[c] *= k;
            c += 1;
        }
        self.am_pulse *= exp_neg(dt * 7.0);
    }

    // ---------------------------------------------------------------- starfield

    fn init_stars(&mut self) {
        self.am_seed = 0x1234_5678;
        let mut i = 0;
        while i < AM_STARS {
            let z = self.random() as f32;
            self.new_star(i, z);
            i += 1;
        }
    }

    /// Star `i` at a random direction and depth `z` (0 = at the camera, 1 = far).
    fn new_star(&mut self, i: usize, z: f32) {
        self.am_stars[i * 3] = (self.random() * 2.0 - 1.0) as f32;
        self.am_stars[i * 3 + 1] = (self.random() * 2.0 - 1.0) as f32;
        self.am_stars[i * 3 + 2] = z;
    }

    /// Next pseudo-random number in 0..1 (linear congruential generator).
    fn random(&mut self) -> f64 {
        self.am_seed = ((self.am_seed as u64 * 1_664_525 + 1_013_904_223) & 0xffff_ffff) as u32;
        (self.am_seed >> 8) as f64 / 16_777_216.0
    }

    /// Stars fly towards the camera, faster when the music is loud.
    fn draw_stars(&mut self, s: &Surface, dt: f64) {
        let speed = (0.15 + 0.6 * self.inten) * dt;
        let (cx, cy) = (s.w as f32 / 2.0, s.h as f32 / 2.0);
        let mut i = 0;
        while i < AM_STARS {
            let z = self.am_stars[i * 3 + 2] - speed as f32;
            if z <= 0.03 {
                self.new_star(i, 1.0);
                i += 1;
                continue;
            }
            self.am_stars[i * 3 + 2] = z;
            let x = cx + self.am_stars[i * 3] / z * cx;
            let y = cy + self.am_stars[i * 3 + 1] / z * cx;
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
        let flash = self.am_flash[c] as f64;
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

    // ---------------------------------------------------------------- logo and scroller

    /// "RUSTRACKER" at the top, rows in cycling copper colours, hopping on beats.
    fn draw_logo(&self, s: &Surface, t: f64) {
        let dot = imax(s.w / 110, 2);
        let x0 = (s.w - 10 * 6 * dot) / 2;
        let y0 = (s.h as f64 * 0.06 - self.am_pulse * dot as f64 * 1.5) as i32;
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
        if self.am_len == 0 {
            return;
        }
        self.am_scroll += dt * 4.5; // characters per second
        if self.am_scroll >= self.am_len as f64 {
            self.am_scroll -= self.am_len as f64;
        }
        let dot = imax(s.h / 52, 2);
        let cw = 6 * dot;
        let base_y = (s.h as f64 * 0.8) as i32 - 7 * dot / 2;
        let amp = s.h as f64 * 0.045;
        let first = self.am_scroll as usize;
        let frac = self.am_scroll - first as f64;
        let visible = s.w / cw + 2;
        let mut k = 0;
        while k < visible {
            let ch = self.am_text[(first + k as usize) % self.am_len] as u32;
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
