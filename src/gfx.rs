//! Framebuffer drawing primitives (RGBA8, one u32 per pixel, laid out as the bytes
//! R, G, B, A in memory, i.e. what a canvas ImageData expects).
//!
//! Core-language-only subset like replayer.rs: a [`Surface`] is a raw pointer plus size
//! and stride, and [`Surface::sub`] makes a clipped window into it, so every view draws
//! in local coordinates. [`TextBuf`] builds text without allocation.

use crate::font;
use crate::math::{iclamp, imax, imin};

/// Builds an opaque colour.
///
/// # Arguments
/// * `r` - red, 0..255
/// * `g` - green, 0..255
/// * `b` - blue, 0..255
///
/// # Returns
/// The colour as u32 (bytes R, G, B, A in memory).
pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    0xff00_0000 | (b << 16) | (g << 8) | r
}
/// Red component of a colour.
///
/// # Arguments
/// * `c` - colour
///
/// # Returns
/// 0..255.
pub fn red(c: u32) -> u32 {
    c & 255
}
/// Green component of a colour.
///
/// # Arguments
/// * `c` - colour
///
/// # Returns
/// 0..255.
pub fn green(c: u32) -> u32 {
    (c >> 8) & 255
}
/// Blue component of a colour.
///
/// # Arguments
/// * `c` - colour
///
/// # Returns
/// 0..255.
pub fn blue(c: u32) -> u32 {
    (c >> 16) & 255
}
/// Linear blend of two colours.
///
/// # Arguments
/// * `a` - first colour
/// * `b` - second colour
/// * `t` - weight of `b`, 0 ..= 256
///
/// # Returns
/// `a` for t = 0, `b` for t = 256, opaque.
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let u = 256 - t;
    rgb(
        (red(a) * u + red(b) * t) >> 8,
        (green(a) * u + green(b) * t) >> 8,
        (blue(a) * u + blue(b) * t) >> 8,
    )
}
/// Scales a colour's brightness.
///
/// # Arguments
/// * `c` - colour
/// * `k` - factor, 0 (black) ..= 256 (unchanged)
///
/// # Returns
/// The darkened colour, opaque.
pub fn dim(c: u32, k: u32) -> u32 {
    rgb((red(c) * k) >> 8, (green(c) * k) >> 8, (blue(c) * k) >> 8)
}
/// Additive blend of two colours, saturating at white (for glow effects).
///
/// # Arguments
/// * `a` - first colour
/// * `b` - second colour
///
/// # Returns
/// The per-component sum, at most 255 each.
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
    /// A surface covering a whole framebuffer (the caller guarantees the memory).
    ///
    /// # Arguments
    /// * `p` - first pixel; `w`·`h` pixels must be writable
    /// * `w` - width in pixels
    /// * `h` - height in pixels
    ///
    /// # Returns
    /// The surface, stride = `w`.
    pub fn new(p: *mut u32, w: i32, h: i32) -> Surface {
        Surface { p, w, h, stride: w }
    }

    /// A clipped window of this surface, for drawing in local coordinates.
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `w` - width in pixels
    /// * `h` - height in pixels
    ///
    /// # Returns
    /// The window; empty (w or h = 0) if it lies outside.
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
    /// Reads a pixel.
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    ///
    /// # Returns
    /// Its colour, or 0 outside the surface.
    #[inline(always)]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return 0;
        }
        unsafe { *self.at(x, y) }
    }
    /// Sets a pixel (ignored outside the surface).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `c` - colour (RGBA as from [`rgb`])
    #[inline(always)]
    pub fn set(&self, x: i32, y: i32, c: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        unsafe {
            *self.at(x, y) = c;
        }
    }
    /// Blends a colour over a pixel (ignored outside the surface).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
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
    /// Adds a colour to a pixel, saturating: glow (ignored outside the surface).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - strength, 0 ..= 256
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

    /// Fills a rectangle (clipped).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `w` - width in pixels
    /// * `h` - height in pixels
    /// * `c` - colour (RGBA as from [`rgb`])
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
    /// Blends a rectangle over the surface (clipped).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `w` - width in pixels
    /// * `h` - height in pixels
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
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
    ///
    /// # Arguments
    /// * `c` - colour (RGBA as from [`rgb`])
    pub fn clear(&self, c: u32) {
        self.fill(0, 0, self.w, self.h, c);
    }
    /// Horizontal line (clipped).
    ///
    /// # Arguments
    /// * `x0` - first column
    /// * `x1` - last column (inclusive)
    /// * `y` - row
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
    pub fn hline(&self, x0: i32, x1: i32, y: i32, c: u32, a: u32) {
        let mut x = x0;
        while x <= x1 {
            self.blend(x, y, c, a);
            x += 1;
        }
    }
    /// Vertical line (clipped).
    ///
    /// # Arguments
    /// * `x` - column
    /// * `y0` - first row
    /// * `y1` - last row (inclusive)
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
    pub fn vline(&self, x: i32, y0: i32, y1: i32, c: u32, a: u32) {
        let mut y = y0;
        while y <= y1 {
            self.blend(x, y, c, a);
            y += 1;
        }
    }
    /// One pixel wide rectangle outline (clipped).
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `w` - width in pixels
    /// * `h` - height in pixels
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
    pub fn rect(&self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        self.hline(x, x + w - 1, y, c, a);
        self.hline(x, x + w - 1, y + h - 1, c, a);
        self.vline(x, y + 1, y + h - 2, c, a);
        self.vline(x + w - 1, y + 1, y + h - 2, c, a);
    }
    /// Oscilloscope-style grid: 8 x 4 faint cells plus a frame, in phosphor green.
    ///
    /// # Arguments
    /// * `x` - column, 0 = left edge
    /// * `y` - row, 0 = top edge
    /// * `w` - width in pixels
    /// * `h` - height in pixels
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

    /// Line with sub-pixel end points, drawn as a DDA (clipped).
    ///
    /// # Arguments
    /// * `x0` - start column
    /// * `y0` - start row
    /// * `x1` - end column
    /// * `y1` - end row
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 (invisible) ..= 256 (opaque)
    #[cfg(not(dos))]
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
    /// Draws a straight line (DOS build): the same pixels as the float version up to rounding,
    /// but stepped in 16.16 fixed point, since every float-to-integer conversion is slow on
    /// the Pentium's FPU. Endpoints are limited to +-16000 pixels.
    ///
    /// # Arguments
    /// * `x0` - start column
    /// * `y0` - start row
    /// * `x1` - end column
    /// * `y1` - end row
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 ..= 256
    #[cfg(dos)]
    pub fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, a: u32) {
        self.line_fix(fix16(x0), fix16(y0), fix16(x1), fix16(y1), c, a);
    }
    /// [`Surface::line`] with endpoints in 16.16 fixed point (DOS build).
    ///
    /// # Arguments
    /// * `fx0` - start column * 65536
    /// * `fy0` - start row * 65536
    /// * `fx1` - end column * 65536
    /// * `fy1` - end row * 65536
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `a` - opacity, 0 ..= 256
    #[cfg(dos)]
    fn line_fix(&self, fx0: i32, fy0: i32, fx1: i32, fy1: i32, c: u32, a: u32) {
        let dx = fx1 - fx0;
        let dy = fy1 - fy0;
        let adx = if dx < 0 { 0 - dx } else { dx };
        let ady = if dy < 0 { 0 - dy } else { dy };
        let n = (if adx > ady { adx } else { ady } >> 16) + 1;
        let sx = dx / n;
        let sy = dy / n;
        let mut x = fx0 + 32768;
        let mut y = fy0 + 32768;
        let mut i = 0;
        while i <= n {
            self.blend(x >> 16, y >> 16, c, a);
            x += sx;
            y += sy;
            i += 1;
        }
    }
    /// A phosphor trace: soft wide glow plus a bright core.
    ///
    /// # Arguments
    /// * `x0` - start column
    /// * `y0` - start row
    /// * `x1` - end column
    /// * `y1` - end row
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `thick` - glow radius in pixels
    #[cfg(not(dos))]
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
    /// A phosphor trace (DOS build): as the float version, with the endpoints converted to
    /// fixed point only once for all its lines.
    ///
    /// # Arguments
    /// * `x0` - start column
    /// * `y0` - start row
    /// * `x1` - end column
    /// * `y1` - end row
    /// * `c` - colour (RGBA as from [`rgb`])
    /// * `thick` - glow radius in pixels
    #[cfg(dos)]
    pub fn trace_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, thick: i32) {
        let (ax, ay, bx, by) = (fix16(x0), fix16(y0), fix16(x1), fix16(y1));
        let mut o = 0 - thick;
        while o <= thick {
            if o != 0 {
                let f = o << 16;
                self.line_fix(ax, ay + f, bx, by + f, c, 46);
                self.line_fix(ax + f, ay, bx + f, by, c, 30);
            }
            o += 1;
        }
        self.line_fix(ax, ay, bx, by, c, 256);
    }

    /// Draws one character of the built-in 5x7 font. Lowercase is drawn as uppercase; 0x80..0x83
    /// are play, pause, stop and a dot, 0xc8 is the U umlaut; unknown bytes become '?'.
    ///
    /// # Arguments
    /// * `x` - left edge
    /// * `y` - top edge
    /// * `ch` - character code
    /// * `scale` - pixels per font dot
    /// * `c` - colour (RGBA as from [`rgb`])
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
    /// Draws a byte string literal.
    ///
    /// # Arguments
    /// * `x` - left edge
    /// * `y` - top edge
    /// * `s` - the text
    /// * `scale` - pixels per font dot (a character is 6·scale wide)
    /// * `c` - colour (RGBA as from [`rgb`])
    ///
    /// # Returns
    /// The x position after the text.
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
    /// Draws the text of a [`TextBuf`].
    ///
    /// # Arguments
    /// * `x` - left edge
    /// * `y` - top edge
    /// * `t` - the text
    /// * `scale` - pixels per font dot (a character is 6·scale wide)
    /// * `c` - colour (RGBA as from [`rgb`])
    ///
    /// # Returns
    /// The x position after the text.
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
///
/// # Arguments
/// * `ch` - character code
///
/// # Returns
/// The glyph code, `font::FIRST ..` (unknown bytes as '?').
pub fn glyph_code(ch: u32) -> u32 {
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
    ///
    /// # Returns
    /// A text with no characters.
    pub const fn new() -> TextBuf {
        TextBuf { b: [0; 96], n: 0 }
    }
    /// Empties the text.
    pub fn clear(&mut self) {
        self.n = 0;
    }
    /// Appends one character (dropped when the text is full).
    ///
    /// # Arguments
    /// * `c` - character code
    pub fn ch(&mut self, c: u8) {
        if self.n < 96 {
            self.b[self.n] = c;
            self.n += 1;
        }
    }
    /// Appends a byte string literal.
    ///
    /// # Arguments
    /// * `s` - the bytes to append
    pub fn s<const N: usize>(&mut self, s: &[u8; N]) {
        let mut i = 0;
        while i < N {
            self.ch(s[i]);
            i += 1;
        }
    }
    /// Appends an unsigned decimal number.
    ///
    /// # Arguments
    /// * `v` - the number
    /// * `width` - minimum number of characters
    /// * `pad` - character used to pad on the left, e.g. b'0' or b' '
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
    /// Appends the lowest hex digits of a number, upper case.
    ///
    /// # Arguments
    /// * `v` - the number
    /// * `digits` - number of digits
    pub fn hex(&mut self, v: u32, digits: u32) {
        let mut i = digits;
        while i > 0 {
            i -= 1;
            self.ch(HEX[((v >> (i * 4)) & 15) as usize]);
        }
    }
    /// Appends a number with one decimal, e.g. "-12.5".
    ///
    /// # Arguments
    /// * `v` - the number (rounded to one decimal)
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
    /// Appends the note name of a ProTracker period, e.g. "C-2".
    ///
    /// # Arguments
    /// * `period` - Amiga period; 0 appends "---"
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
    /// Appends the name of a MIDI note, e.g. "E4" or "C#3".
    ///
    /// # Arguments
    /// * `m` - MIDI note number (69 = A4)
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

/// A coordinate in 16.16 fixed point, limited to +-16000 (DOS line drawing).
///
/// # Arguments
/// * `v` - coordinate in pixels
///
/// # Returns
/// `v * 65536`, rounded towards zero
#[cfg(dos)]
fn fix16(v: f32) -> i32 {
    let c = if v > 16000.0 {
        16000.0
    } else if v < -16000.0 {
        -16000.0
    } else {
        v
    };
    (c * 65536.0) as i32
}
