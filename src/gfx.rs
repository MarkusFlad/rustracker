//! Framebuffer drawing primitives (RGBA8, one u32 per pixel, laid out as the bytes
//! R, G, B, A in memory, i.e. what a canvas ImageData expects).
//!
//! Core-language-only subset like replayer.rs: a [`Surface`] is a raw pointer plus size
//! and stride, and [`Surface::sub`] makes a clipped window into it, so every view draws
//! in local coordinates. [`TextBuf`] builds text without allocation.

use crate::font;
use crate::math::{iclamp, imax, imin};

/// Opaque colour from 8-bit red, green and blue.
pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    0xff00_0000 | (b << 16) | (g << 8) | r
}
/// Red component (0..255).
pub fn red(c: u32) -> u32 {
    c & 255
}
/// Green component (0..255).
pub fn green(c: u32) -> u32 {
    (c >> 8) & 255
}
/// Blue component (0..255).
pub fn blue(c: u32) -> u32 {
    (c >> 16) & 255
}
/// Mix two colours, t = 0..256 (0 = a, 256 = b).
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let u = 256 - t;
    rgb(
        (red(a) * u + red(b) * t) >> 8,
        (green(a) * u + green(b) * t) >> 8,
        (blue(a) * u + blue(b) * t) >> 8,
    )
}
/// Scale a colour's brightness, k = 0..256.
pub fn dim(c: u32, k: u32) -> u32 {
    rgb((red(c) * k) >> 8, (green(c) * k) >> 8, (blue(c) * k) >> 8)
}
/// Additive blend of two colours, saturating at white.
pub fn add(a: u32, b: u32) -> u32 {
    let r = red(a) + red(b);
    let g = green(a) + green(b);
    let bl = blue(a) + blue(b);
    rgb(
        if r > 255 { 255 } else { r },
        if g > 255 { 255 } else { g },
        if bl > 255 { 255 } else { bl },
    )
}

/// Black.
pub const BLACK: u32 = rgb(0, 0, 0);
/// Background of the views.
pub const BG: u32 = rgb(2, 5, 4);
/// Background of the bar and the panel.
pub const PANEL: u32 = rgb(10, 15, 14);
/// Separator lines.
pub const LINE: u32 = rgb(24, 40, 34);
/// Oscilloscope green, the main accent.
pub const PHOSPHOR: u32 = rgb(57, 255, 136);
/// Dim green for labels and inactive elements.
pub const PHOS_DIM: u32 = rgb(27, 122, 69);
/// Amber, the second accent (right channel, song title).
pub const AMBER: u32 = rgb(255, 180, 58);
/// Normal text.
pub const TEXT: u32 = rgb(185, 216, 198);
/// Secondary text.
pub const MUTED: u32 = rgb(95, 122, 108);
/// Highlights.
pub const WHITE: u32 = rgb(230, 245, 236);
/// Overload (top of the spectrum bars).
pub const RED: u32 = rgb(255, 90, 58);

/// A rectangular window into a framebuffer. All drawing functions clip to it.
pub struct Surface {
    /// Top left pixel.
    pub p: *mut u32,
    /// Width in pixels.
    pub w: i32,
    /// Height in pixels.
    pub h: i32,
    /// Pixels from one row to the next.
    pub stride: i32,
}

impl Surface {
    /// A whole framebuffer of `w` x `h` pixels at `p` (the caller guarantees the memory).
    pub fn new(p: *mut u32, w: i32, h: i32) -> Surface {
        Surface { p, w, h, stride: w }
    }

    /// A clipped window (x, y, w, h) of this surface; empty if it lies outside.
    pub fn sub(&self, x: i32, y: i32, w: i32, h: i32) -> Surface {
        let x0 = iclamp(x, 0, self.w);
        let y0 = iclamp(y, 0, self.h);
        let x1 = imax(imin(x + w, self.w), x0);
        let y1 = imax(imin(y + h, self.h), y0);
        let off = (y0 * self.stride + x0) as usize;
        Surface {
            p: (self.p as usize + off * 4) as *mut u32,
            w: x1 - x0,
            h: y1 - y0,
            stride: self.stride,
        }
    }

    #[inline(always)]
    fn at(&self, x: i32, y: i32) -> *mut u32 {
        (self.p as usize + ((y * self.stride + x) as usize) * 4) as *mut u32
    }
    /// Pixel (x, y), 0 outside.
    #[inline(always)]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return 0;
        }
        unsafe { *self.at(x, y) }
    }
    /// Sets pixel (x, y) unless it is outside.
    #[inline(always)]
    pub fn set(&self, x: i32, y: i32, c: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        unsafe {
            *self.at(x, y) = c;
        }
    }
    /// Blend colour c over the pixel, a = 0..256.
    #[inline(always)]
    pub fn blend(&self, x: i32, y: i32, c: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        unsafe {
            let p = self.at(x, y);
            *p = if a >= 256 { c } else { mix(*p, c, a) };
        }
    }
    /// Add colour c (scaled by a = 0..256) to the pixel: glow.
    #[inline(always)]
    pub fn glow(&self, x: i32, y: i32, c: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        unsafe {
            let p = self.at(x, y);
            *p = add(*p, dim(c, a));
        }
    }

    /// Fills a rectangle.
    pub fn fill(&self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let s = self.sub(x, y, w, h);
        let mut yy = 0;
        while yy < s.h {
            let mut xx = 0;
            let row = s.at(0, yy) as usize;
            while xx < s.w {
                unsafe {
                    *((row + (xx as usize) * 4) as *mut u32) = c;
                }
                xx += 1;
            }
            yy += 1;
        }
    }
    /// Blends a rectangle with opacity `a` (0..256).
    pub fn fill_a(&self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        let s = self.sub(x, y, w, h);
        let mut yy = 0;
        while yy < s.h {
            let mut xx = 0;
            while xx < s.w {
                s.blend(xx, yy, c, a);
                xx += 1;
            }
            yy += 1;
        }
    }
    /// Fills the whole surface.
    pub fn clear(&self, c: u32) {
        self.fill(0, 0, self.w, self.h, c);
    }
    /// Horizontal line from x0 to x1 (inclusive) with opacity `a`.
    pub fn hline(&self, x0: i32, x1: i32, y: i32, c: u32, a: u32) {
        let mut x = x0;
        while x <= x1 {
            self.blend(x, y, c, a);
            x += 1;
        }
    }
    /// Vertical line from y0 to y1 (inclusive) with opacity `a`.
    pub fn vline(&self, x: i32, y0: i32, y1: i32, c: u32, a: u32) {
        let mut y = y0;
        while y <= y1 {
            self.blend(x, y, c, a);
            y += 1;
        }
    }
    /// One pixel wide rectangle outline with opacity `a`.
    pub fn rect(&self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        self.hline(x, x + w - 1, y, c, a);
        self.hline(x, x + w - 1, y + h - 1, c, a);
        self.vline(x, y + 1, y + h - 2, c, a);
        self.vline(x + w - 1, y + 1, y + h - 2, c, a);
    }
    /// A grid of 8 x 4 cells plus a frame, in the oscilloscope style.
    pub fn grid(&self, x: i32, y: i32, w: i32, h: i32) {
        let mut i = 1;
        while i < 8 {
            self.vline(x + w * i / 8, y, y + h - 1, PHOSPHOR, 18);
            i += 1;
        }
        i = 1;
        while i < 4 {
            self.hline(x, x + w - 1, y + h * i / 4, PHOSPHOR, 18);
            i += 1;
        }
        self.rect(x, y, w, h, PHOSPHOR, 64);
    }

    /// Line with sub-pixel end points, drawn as a DDA; `a` is the opacity (0..256).
    pub fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, a: u32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let adx = if dx < 0.0 { 0.0 - dx } else { dx };
        let ady = if dy < 0.0 { 0.0 - dy } else { dy };
        let n = (if adx > ady { adx } else { ady }) as i32 + 1;
        let sx = dx / n as f32;
        let sy = dy / n as f32;
        let mut x = x0;
        let mut y = y0;
        let mut i = 0;
        while i <= n {
            self.blend(
                (x + 0.5) as i32 - if x < -0.5 { 1 } else { 0 },
                (y + 0.5) as i32,
                c,
                a,
            );
            x += sx;
            y += sy;
            i += 1;
        }
    }
    /// A phosphor trace: soft wide glow plus a bright core.
    pub fn trace_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, thick: i32) {
        let mut o = 0 - thick;
        while o <= thick {
            if o != 0 {
                self.line(x0, y0 + o as f32, x1, y1 + o as f32, c, 46);
                self.line(x0 + o as f32, y0, x1 + o as f32, y1, c, 30);
            }
            o += 1;
        }
        self.line(x0, y0, x1, y1, c, 256);
    }

    /// Draw one glyph (byte) at (x, y), scale = pixel size of one font dot.
    /// Lowercase is drawn as uppercase; 0x80..0x83 are play, pause, stop and a dot,
    /// 0xc8 is the U umlaut; unknown bytes become '?'.
    pub fn glyph(&self, x: i32, y: i32, ch: u32, scale: i32, c: u32) {
        let base = ((glyph_code(ch) - font::FIRST) * 7) as usize;
        let mut r = 0;
        while r < 7 {
            let bits = font::GLYPHS[base + r as usize] as u32;
            let mut col: i32 = 0;
            while col < 5 {
                if (bits >> (4 - col) as u32) & 1 != 0 {
                    self.fill(x + col * scale, y + r * scale, scale, scale, c);
                }
                col += 1;
            }
            r += 1;
        }
    }
    /// Text from a byte string literal; returns the x after the text.
    pub fn text<const N: usize>(&self, x: i32, y: i32, s: &[u8; N], scale: i32, c: u32) -> i32 {
        let mut i = 0;
        let mut xx = x;
        while i < N {
            self.glyph(xx, y, s[i] as u32, scale, c);
            xx += 6 * scale;
            i += 1;
        }
        xx
    }
    /// Text from a [`TextBuf`]; returns the x after the text.
    pub fn text_buf(&self, x: i32, y: i32, t: &TextBuf, scale: i32, c: u32) -> i32 {
        let mut i = 0;
        let mut xx = x;
        while i < t.n {
            self.glyph(xx, y, t.b[i] as u32, scale, c);
            xx += 6 * scale;
            i += 1;
        }
        xx
    }
}

/// Maps a byte onto the font's code range (see [`Surface::glyph`]).
fn glyph_code(ch: u32) -> u32 {
    let mut code = ch;
    if code >= 97 && code <= 122 {
        code -= 32; // lowercase -> uppercase
    }
    if code >= 0x80 && code <= 0x83 {
        code = 96 + (code - 0x80); // play, pause, stop, dot
    }
    if code == 0xc8 {
        code = 100; // U-umlaut
    }
    if code < font::FIRST || code >= font::FIRST + font::COUNT {
        code = 63; // '?'
    }
    code
}

/// Fixed-size text builder (no allocation); characters beyond 96 are dropped.
pub struct TextBuf {
    /// The characters.
    pub b: [u8; 96],
    /// Number of characters used.
    pub n: usize,
}

const NOTE_NAMES: &[u8; 24] = b"C-C#D-D#E-F-F#G-G#A-A#B-";
const HEX: &[u8; 16] = b"0123456789ABCDEF";

impl TextBuf {
    /// An empty text.
    pub const fn new() -> TextBuf {
        TextBuf { b: [0; 96], n: 0 }
    }
    /// Empties the text.
    pub fn clear(&mut self) {
        self.n = 0;
    }
    /// Appends one byte.
    pub fn ch(&mut self, c: u8) {
        if self.n < 96 {
            self.b[self.n] = c;
            self.n += 1;
        }
    }
    /// Appends a byte string literal.
    pub fn s<const N: usize>(&mut self, s: &[u8; N]) {
        let mut i = 0;
        while i < N {
            self.ch(s[i]);
            i += 1;
        }
    }
    /// Unsigned number, at least `width` digits, padded with `pad`.
    pub fn num(&mut self, v: u32, width: usize, pad: u8) {
        let mut d = [0u8; 10];
        let mut n = 0;
        let mut x = v;
        while n == 0 || x > 0 {
            d[n] = 48 + (x % 10) as u8;
            x /= 10;
            n += 1;
        }
        while n < width {
            d[n] = pad;
            n += 1;
        }
        while n > 0 {
            n -= 1;
            self.ch(d[n]);
        }
    }
    /// The lowest `digits` hex digits of `v`, upper case.
    pub fn hex(&mut self, v: u32, digits: u32) {
        let mut i = digits;
        while i > 0 {
            i -= 1;
            self.ch(HEX[((v >> (i * 4)) & 15) as usize]);
        }
    }
    /// Fixed point number with one decimal.
    pub fn dec1(&mut self, v: f64) {
        let neg = v < 0.0;
        let a = if neg { 0.0 - v } else { v };
        let t = (a * 10.0 + 0.5) as u32;
        if neg {
            self.ch(45);
        }
        self.num(t / 10, 1, 48);
        self.ch(46);
        self.ch(48 + (t % 10) as u8);
    }
    /// Note name of a ProTracker period ("C-2"), "---" for 0.
    pub fn note(&mut self, period: u32) {
        if period == 0 {
            self.s(b"---");
            return;
        }
        let i = crate::replayer::nearest_note(period as i32);
        self.ch(NOTE_NAMES[(i % 12) * 2]);
        self.ch(NOTE_NAMES[(i % 12) * 2 + 1]);
        self.ch(49 + (i / 12) as u8);
    }
    /// Note name for a MIDI note number ("E4").
    pub fn midi(&mut self, m: i32) {
        let k = ((m % 12) + 12) % 12;
        self.ch(NOTE_NAMES[(k * 2) as usize]);
        if NOTE_NAMES[(k * 2 + 1) as usize] == 35 {
            self.ch(35);
        }
        let o = m / 12 - 1;
        if o < 0 {
            self.ch(45);
            self.num((0 - o) as u32, 1, 48);
        } else {
            self.num(o as u32, 1, 48);
        }
    }
}
