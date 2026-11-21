//! ProTracker MOD replayer: parser, effects and mixer.
//!
//! This file deliberately uses only the core language: `while` loops, arrays,
//! structs, operators and casts. No iterators, no Option, no slices, no methods
//! from `core` and no allocation. That way the very same source compiles as a
//! normal Rust module (native tests, `cargo build --target wasm32-unknown-unknown`)
//! and inside a minimal `#![no_core]` environment (see `preview/minicore.rs`).
//!
//! The MOD file is never copied or converted: patterns and samples are read
//! straight from the caller's byte buffer (passed as a raw pointer + length).
//!
//! Usage: [`Player::empty`], [`Player::load`], then [`Player::render`] repeatedly.

use crate::math::{iabs, iclamp, imax, imin, umin};

/// Most channels a module may have.
pub const MAX_CH: usize = 32;
/// Rows per pattern.
pub const ROWS: usize = 64;
/// Amiga PAL clock: playback rate of a sample = PAL_CLOCK / (2 · period).
const PAL_CLOCK: f64 = 7093789.2;
/// Period limits of ProTracker's slides (B-3 and C-1).
const PERIOD_MIN: i32 = 113;
const PERIOD_MAX: i32 = 856;

/// ProTracker periods for the notes C-1 .. B-3 at finetune 0 (index 0 = C-1).
pub const BASE_PERIODS: [i32; 36] = [
    856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453, //
    428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226, //
    214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113,
];

/// Periods per finetune nibble (0..7 = +0..+7, 8..15 = -8..-1), 36 notes each.
const FT_PERIODS: [i32; 576] = [
    856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453, 428, 404, 381, 360, 339, 320, 302,
    285, 269, 254, 240, 226, 214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113, 850, 802,
    757, 715, 673, 635, 600, 566, 534, 504, 477, 450, 425, 401, 378, 357, 337, 318, 300, 283, 267,
    252, 238, 224, 212, 201, 189, 179, 169, 159, 150, 142, 134, 126, 119, 112, 844, 797, 752, 709,
    669, 631, 595, 562, 530, 501, 473, 447, 422, 398, 376, 355, 335, 316, 298, 281, 265, 250, 237,
    223, 211, 199, 187, 178, 168, 158, 149, 141, 133, 125, 118, 111, 838, 791, 746, 704, 664, 627,
    591, 558, 527, 497, 470, 443, 419, 395, 373, 352, 332, 313, 296, 279, 263, 249, 235, 221, 210,
    198, 186, 176, 166, 157, 148, 140, 132, 124, 118, 111, 832, 785, 741, 699, 660, 623, 587, 554,
    523, 494, 466, 440, 416, 392, 370, 350, 330, 311, 294, 277, 262, 247, 233, 220, 208, 196, 185,
    175, 165, 156, 147, 139, 131, 123, 117, 110, 826, 779, 736, 694, 655, 619, 583, 550, 520, 490,
    463, 437, 413, 390, 368, 347, 327, 309, 292, 275, 260, 245, 232, 218, 207, 195, 184, 174, 164,
    155, 146, 138, 130, 123, 116, 109, 820, 774, 730, 689, 651, 614, 579, 546, 516, 487, 460, 434,
    410, 387, 365, 345, 325, 307, 290, 273, 258, 243, 230, 217, 205, 193, 182, 172, 163, 154, 145,
    137, 129, 122, 115, 108, 814, 768, 725, 684, 646, 610, 575, 543, 513, 484, 457, 431, 407, 384,
    363, 342, 323, 305, 288, 271, 256, 242, 228, 215, 204, 192, 181, 171, 162, 153, 144, 137, 129,
    121, 115, 108, 907, 856, 807, 763, 718, 678, 640, 604, 570, 538, 509, 480, 453, 428, 404, 381,
    359, 339, 320, 302, 285, 269, 254, 239, 227, 214, 201, 191, 180, 170, 160, 152, 143, 135, 127,
    120, 900, 850, 802, 757, 713, 673, 635, 600, 566, 534, 505, 476, 450, 425, 401, 379, 357, 337,
    318, 300, 283, 267, 252, 238, 225, 212, 200, 189, 179, 169, 159, 150, 142, 134, 126, 119, 894,
    844, 796, 752, 708, 668, 631, 595, 562, 530, 501, 473, 447, 422, 398, 376, 354, 334, 316, 298,
    281, 265, 250, 236, 223, 211, 199, 188, 177, 167, 158, 149, 141, 133, 125, 118, 887, 838, 791,
    746, 703, 664, 626, 591, 558, 527, 498, 470, 444, 419, 395, 373, 352, 332, 313, 296, 279, 263,
    249, 234, 222, 209, 197, 187, 176, 166, 157, 148, 140, 132, 124, 117, 881, 832, 785, 741, 698,
    659, 622, 587, 554, 523, 494, 466, 441, 416, 392, 370, 349, 330, 311, 293, 277, 261, 247, 233,
    220, 208, 196, 185, 175, 165, 155, 147, 139, 131, 124, 116, 875, 826, 779, 736, 693, 654, 617,
    582, 550, 519, 491, 463, 437, 413, 390, 368, 347, 327, 309, 291, 275, 260, 245, 231, 219, 206,
    194, 184, 174, 164, 154, 146, 138, 130, 123, 115, 868, 820, 774, 730, 688, 650, 613, 578, 546,
    516, 487, 460, 434, 410, 387, 365, 344, 325, 306, 289, 273, 258, 243, 229, 217, 205, 193, 183,
    172, 163, 153, 145, 137, 129, 122, 115, 862, 814, 768, 725, 683, 645, 609, 575, 542, 512, 484,
    457, 431, 407, 384, 363, 342, 323, 305, 287, 271, 256, 242, 228, 216, 203, 192, 181, 171, 161,
    152, 144, 136, 128, 121, 114,
];

const SINE: [i32; 32] = [
    0, 24, 49, 74, 97, 120, 141, 161, 180, 197, 212, 224, 235, 244, 250, 253, //
    255, 253, 250, 244, 235, 224, 212, 197, 180, 161, 141, 120, 97, 74, 49, 24,
];

// ------------------------------------------------------------------ memory access

#[inline(always)]
fn rd(m: *const u8, i: usize) -> u32 {
    unsafe { *((m as usize + i) as *const u8) as u32 }
}

#[inline(always)]
fn rd_i8(m: *const u8, i: usize) -> f32 {
    unsafe { *((m as usize + i) as *const u8) as i8 as f32 }
}

#[inline(always)]
fn wr(p: *mut f32, i: usize, v: f32) {
    unsafe {
        *((p as usize + i * 4) as *mut f32) = v;
    }
}

#[inline(always)]
fn add_to(p: *mut f32, i: usize, v: f32) {
    unsafe {
        *((p as usize + i * 4) as *mut f32) += v;
    }
}

fn be16(m: *const u8, o: usize) -> usize {
    ((rd(m, o) << 8) | rd(m, o + 1)) as usize
}

fn fill(p: *mut f32, n: usize, v: f32) {
    let mut i = 0;
    while i < n {
        wr(p, i, v);
        i += 1;
    }
}

// ------------------------------------------------------------------ notes and waveforms

fn is_digit(c: u32) -> bool {
    c >= 48 && c <= 57
}

fn ft_period(note: usize, ft_nibble: u32) -> i32 {
    let n = if note > 35 { 35 } else { note };
    FT_PERIODS[(ft_nibble & 15) as usize * 36 + n]
}

/// Finds the note closest to a ProTracker period (finetune 0).
///
/// # Arguments
/// * `period` - Amiga period, e.g. 428 for C-2
///
/// # Returns
/// Note index 0 (C-1) .. 35 (B-3).
pub fn nearest_note(period: i32) -> usize {
    nearest_note_ft(period, 0)
}

/// Index of the note whose period at finetune `ft` is closest to `period`.
fn nearest_note_ft(period: i32, ft: u32) -> usize {
    let mut best = 0;
    let mut diff = 1 << 30;
    let mut i = 0;
    while i < 36 {
        let d = iabs(ft_period(i, ft) - period);
        if d < diff {
            diff = d;
            best = i;
        }
        i += 1;
    }
    best
}

/// Vibrato/tremolo waveform (0 sine, 1 ramp down, 2/3 square) at position 0..63, ±255.
fn wave_value(wave: u32, pos: u32) -> i32 {
    let p = (pos & 63) as i32;
    let w = wave & 3;
    if w == 0 {
        let v = SINE[(p & 31) as usize];
        if p >= 32 {
            0 - v
        } else {
            v
        }
    } else if w == 1 {
        255 - p * 8
    } else if p >= 32 {
        -255
    } else {
        255
    }
}

/// Number of channels for the 4-byte format tag at offset 1080, or 0 if the tag is
/// unknown (then the file is an old 15-sample module with 4 channels).
fn channels_from_tag(a: u32, b: u32, c: u32, d: u32) -> usize {
    let tag = (a << 24) | (b << 16) | (c << 8) | d;
    // 'M.K.' 'M!K!' 'M&K!' 'N.T.' 'FLT4' '4CHN'
    if tag == 0x4D2E4B2E
        || tag == 0x4D214B21
        || tag == 0x4D264B21
        || tag == 0x4E2E542E
        || tag == 0x464C5434
        || tag == 0x3443484E
    {
        return 4;
    }
    if tag == 0x3643484E {
        return 6; // '6CHN'
    }
    // '8CHN' 'FLT8' 'OCTA' 'CD81'
    if tag == 0x3843484E || tag == 0x464C5438 || tag == 0x4F435441 || tag == 0x43443831 {
        return 8;
    }
    if is_digit(a) && is_digit(b) && c == 67 && d == 72 {
        return ((a - 48) * 10 + (b - 48)) as usize; // 'xxCH'
    }
    if is_digit(a) && b == 67 && c == 72 && d == 78 {
        return (a - 48) as usize; // 'xCHN'
    }
    0
}

// ------------------------------------------------------------------ pattern cells

/// One pattern cell: sample number, note period and effect.
pub struct Cell {
    /// Sample number 1..31, 0 = none.
    pub sample: u32,
    /// Amiga period of the note, 0 = no note.
    pub period: u32,
    /// Effect command 0x0 .. 0xF.
    pub effect: u32,
    /// Effect parameter byte.
    pub param: u32,
}

impl Cell {
    /// An empty cell.
    pub const EMPTY: Cell = Cell {
        sample: 0,
        period: 0,
        effect: 0,
        param: 0,
    };

    /// Decodes a cell as stored in a MOD pattern.
    ///
    /// # Arguments
    /// * `b0` - byte 0: sample high nibble, period bits 8..11
    /// * `b1` - byte 1: period bits 0..7
    /// * `b2` - byte 2: sample low nibble, effect
    /// * `b3` - byte 3: effect parameter
    ///
    /// # Returns
    /// The decoded cell.
    pub fn decode(b0: u32, b1: u32, b2: u32, b3: u32) -> Cell {
        Cell {
            sample: (b0 & 0xF0) | (b2 >> 4),
            period: ((b0 & 15) << 8) | b1,
            effect: b2 & 15,
            param: b3,
        }
    }
    /// High nibble of the effect parameter.
    ///
    /// # Returns
    /// `param >> 4`, 0..15.
    pub fn x(&self) -> u32 {
        self.param >> 4
    }
    /// Low nibble of the effect parameter.
    ///
    /// # Returns
    /// `param & 15`, 0..15.
    pub fn y(&self) -> u32 {
        self.param & 15
    }
    /// Whether the cell delays its note (EDx with x > 0: the note starts later in the row).
    ///
    /// # Returns
    /// `true` for a note delay.
    pub fn is_note_delay(&self) -> bool {
        self.effect == 0xE && self.x() == 0xD && self.y() > 0
    }
    /// A copy (Cell is not `Copy` in the core-only subset).
    ///
    /// # Returns
    /// An identical cell.
    pub fn copy(&self) -> Cell {
        Cell {
            sample: self.sample,
            period: self.period,
            effect: self.effect,
            param: self.param,
        }
    }
}

// ------------------------------------------------------------------ samples

/// Where a sample's data lies in the MOD buffer, plus its loop and defaults.
struct SampleInfo {
    off: usize,
    len: usize,
    loop_start: usize,
    loop_len: usize,
    volume: i32,
    finetune: u32,
}

const SAMPLE0: SampleInfo = SampleInfo {
    off: 0,
    len: 0,
    loop_start: 0,
    loop_len: 0,
    volume: 0,
    finetune: 0,
};

impl SampleInfo {
    /// Reads the 30-byte header at `o`; the data starts at `start` in a file of `len` bytes.
    fn read(m: *const u8, o: usize, start: usize, len: usize) -> SampleInfo {
        let slen = be16(m, o + 22) * 2;
        let avail = if start < len {
            umin(slen, len - start)
        } else {
            0
        };
        let mut ls = be16(m, o + 26) * 2;
        let mut ll = be16(m, o + 28) * 2;
        if ls > avail {
            ls = avail;
        }
        if ls + ll > avail {
            ll = avail - ls;
        }
        if ll <= 2 {
            ls = 0;
            ll = 0;
        }
        let v = rd(m, o + 25) as i32;
        SampleInfo {
            off: start,
            len: avail,
            loop_start: ls,
            loop_len: ll,
            volume: imin(v, 64),
            finetune: rd(m, o + 24) & 15,
        }
    }
}

// ------------------------------------------------------------------ channels

/// Playback state of one channel.
pub struct Channel {
    sample: usize, // 1-based, 0 = none
    active: bool,
    pos: f64,
    period: i32,
    out_period: i32,
    target: i32,
    porta_speed: i32,
    volume: i32,
    out_volume: i32,
    finetune: u32, // nibble
    gain: f32,
    pan: f32,
    effect: u32,
    param: u32,
    vib_pos: u32,
    vib_speed: u32,
    vib_depth: u32,
    vib_wave: u32,
    trem_pos: u32,
    trem_speed: u32,
    trem_depth: u32,
    trem_wave: u32,
    offset_mem: u32,
    loop_row: usize,
    loop_count: u32,
    has_delayed: bool,
    delayed: Cell,
}

const CHANNEL_INIT: Channel = Channel {
    sample: 0,
    active: false,
    pos: 0.0,
    period: 0,
    out_period: 0,
    target: 0,
    porta_speed: 0,
    volume: 0,
    out_volume: 0,
    finetune: 0,
    gain: 0.0,
    pan: 0.5,
    effect: 0,
    param: 0,
    vib_pos: 0,
    vib_speed: 0,
    vib_depth: 0,
    vib_wave: 0,
    trem_pos: 0,
    trem_speed: 0,
    trem_depth: 0,
    trem_wave: 0,
    offset_mem: 0,
    loop_row: 0,
    loop_count: 0,
    has_delayed: false,
    delayed: Cell::EMPTY,
};

impl Channel {
    /// The values used by the mixer for this tick equal the base values.
    fn latch(&mut self) {
        self.out_period = self.period;
        self.out_volume = self.volume;
    }

    /// Starts the note of `cell` (period already finetuned) from the beginning or,
    /// with effect 9, from a sample offset. `smp_len` is the current sample's length.
    fn start_note(&mut self, cell: &Cell, per: i32, smp_len: usize) {
        self.period = per;
        self.pos = 0.0;
        self.active = self.sample > 0;
        if self.vib_wave < 4 {
            self.vib_pos = 0;
        }
        if self.trem_wave < 4 {
            self.trem_pos = 0;
        }
        if cell.effect == 9 {
            if cell.param > 0 {
                self.offset_mem = cell.param;
            }
            self.pos = (self.offset_mem * 256) as f64;
            if self.pos >= smp_len as f64 {
                self.active = false;
            }
        }
    }

    /// Effects evaluated once on the first tick of a row (those that only touch this channel).
    fn row_effect(&mut self, cell: &Cell) {
        let (e, x, y) = (cell.effect, cell.x(), cell.y());
        if e == 0x3 {
            if cell.param > 0 {
                self.porta_speed = cell.param as i32;
            }
        } else if e == 0x4 {
            if x > 0 {
                self.vib_speed = x;
            }
            if y > 0 {
                self.vib_depth = y;
            }
        } else if e == 0x7 {
            if x > 0 {
                self.trem_speed = x;
            }
            if y > 0 {
                self.trem_depth = y;
            }
        } else if e == 0x8 {
            self.pan = cell.param as f32 / 255.0;
        } else if e == 0xC {
            self.volume = imin(cell.param as i32, 64);
        } else if e == 0xE {
            self.extended_row_effect(x, y);
        }
    }

    /// E1x/E2x fine slides, E4x/E7x waveforms, EAx/EBx fine volume, EC0 cut.
    fn extended_row_effect(&mut self, x: u32, y: u32) {
        if x == 0x1 {
            self.period = imax(self.period - y as i32, PERIOD_MIN);
        } else if x == 0x2 {
            self.period = imin(self.period + y as i32, PERIOD_MAX);
        } else if x == 0x4 {
            self.vib_wave = y;
        } else if x == 0x7 {
            self.trem_wave = y;
        } else if x == 0xA {
            self.volume = imin(self.volume + y as i32, 64);
        } else if x == 0xB {
            self.volume = imax(self.volume - y as i32, 0);
        } else if x == 0xC && y == 0 {
            self.volume = 0;
        }
    }

    /// Effects of the ticks after the first one (everything except note delay).
    fn tick_effect(&mut self, tick: u32) {
        let e = self.effect;
        self.out_period = self.period;
        if e == 0x7 {
            self.tremolo();
            return;
        }
        let vol_slide = if e == 0x0 {
            self.arpeggio(tick);
            false
        } else if e == 0x1 {
            self.slide(0 - self.param as i32);
            false
        } else if e == 0x2 {
            self.slide(self.param as i32);
            false
        } else if e == 0x3 || e == 0x5 {
            self.tone_porta();
            e == 0x5
        } else if e == 0x4 || e == 0x6 {
            self.vibrato();
            e == 0x6
        } else if e == 0xE {
            self.extended_tick(tick);
            false
        } else {
            e == 0xA
        };
        if vol_slide {
            self.volume_slide();
        }
        self.out_volume = self.volume;
    }

    /// 0xy: cycle between the note, +x and +y semitones.
    fn arpeggio(&mut self, tick: u32) {
        if self.param == 0 {
            return;
        }
        let t = tick % 3;
        let add = if t == 1 {
            (self.param >> 4) as usize
        } else if t == 2 {
            (self.param & 15) as usize
        } else {
            0
        };
        let base = nearest_note_ft(self.period, self.finetune);
        self.out_period = ft_period(base + add, self.finetune);
    }

    /// 1xx/2xx: period slide by `delta` per tick, within ProTracker's limits.
    fn slide(&mut self, delta: i32) {
        self.period = if delta < 0 {
            imax(self.period + delta, PERIOD_MIN)
        } else {
            imin(self.period + delta, PERIOD_MAX)
        };
        self.out_period = self.period;
    }

    /// 3xx/5xy: slide towards the target note.
    fn tone_porta(&mut self) {
        if self.target != 0 {
            if self.period < self.target {
                self.period = imin(self.period + self.porta_speed, self.target);
            } else if self.period > self.target {
                self.period = imax(self.period - self.porta_speed, self.target);
            }
        }
        self.out_period = self.period;
    }

    /// 4xy/6xy: periodic pitch modulation.
    fn vibrato(&mut self) {
        let d = wave_value(self.vib_wave, self.vib_pos) * self.vib_depth as i32 / 128;
        self.out_period = imax(self.period + d, 1);
        self.vib_pos = (self.vib_pos + self.vib_speed) & 63;
    }

    /// 7xy: periodic volume modulation (does not change the base volume).
    fn tremolo(&mut self) {
        let d = wave_value(self.trem_wave, self.trem_pos) * self.trem_depth as i32 / 64;
        self.out_volume = iclamp(self.volume + d, 0, 64);
        self.trem_pos = (self.trem_pos + self.trem_speed) & 63;
    }

    /// Axy (and the volume part of 5xy/6xy): up by x or down by y per tick.
    fn volume_slide(&mut self) {
        let (x, y) = (self.param >> 4, self.param & 15);
        if x > 0 {
            self.volume = imin(self.volume + x as i32, 64);
        } else {
            self.volume = imax(self.volume - y as i32, 0);
        }
    }

    /// E9x retrigger and ECx note cut.
    fn extended_tick(&mut self, tick: u32) {
        let (x, y) = (self.param >> 4, self.param & 15);
        if x == 0x9 {
            if y > 0 && tick % y == 0 {
                self.pos = 0.0;
                self.active = self.sample > 0;
            }
        } else if x == 0xC && tick == y {
            self.volume = 0;
        }
    }
}

// ------------------------------------------------------------------ mixer

/// Per-call mixing constants of one channel.
struct MixSetup {
    off: usize,
    len: usize,
    loop_start: usize,
    loop_len: usize,
    looped: bool,
    loop_end: f64,
    step: f64,
    target: f32,
    gl: f32,
    gr: f32,
}

impl MixSetup {
    /// The interpolated sample value at the channel's position (linear interpolation,
    /// the next sample wraps into the loop).
    #[inline(always)]
    fn sample_at(&self, m: *const u8, pos: f64) -> f32 {
        let ip = pos as usize;
        let frac = (pos - ip as f64) as f32;
        let a = if ip < self.len {
            rd_i8(m, self.off + ip)
        } else {
            0.0
        };
        let nx = ip + 1;
        let b = if self.looped && nx as f64 >= self.loop_end {
            rd_i8(m, self.off + self.loop_start)
        } else if nx < self.len {
            rd_i8(m, self.off + nx)
        } else {
            0.0
        };
        a + (b - a) * frac
    }

    /// Advances the position by one output sample; returns false when a one-shot
    /// sample has ended.
    #[inline(always)]
    fn advance(&self, pos: &mut f64) -> bool {
        *pos += self.step;
        if self.looped {
            while *pos >= self.loop_end {
                *pos -= self.loop_len as f64;
            }
        } else if *pos >= self.len as f64 {
            return false;
        }
        true
    }
}

// ------------------------------------------------------------------ player

/// A MOD player: song data (read in place from the caller's buffer), sequencer
/// position and channel states.
pub struct Player {
    m: *const u8,
    mod_len: usize,
    /// Number of channels of the loaded module.
    pub channels: usize,
    num_samples: usize,
    samples: [SampleInfo; 32],
    /// Number of positions in the order list.
    pub song_len: usize,
    restart: usize,
    order: [u32; 128],
    pat_base: usize,
    num_patterns: usize,
    sample_rate: f64,
    ch: [Channel; MAX_CH],
    /// Ticks per row (Fxx with xx < 32).
    pub speed: u32,
    /// Tempo in BPM (Fxx with xx >= 32); one tick lasts 2.5 / tempo seconds.
    pub tempo: u32,
    tick: u32,
    /// Current position in the order list.
    pub order_pos: usize,
    /// Current row in the pattern.
    pub row: usize,
    break_row: i32, // -1 = none
    jump_order: i32,
    loop_jump: i32,
    pat_delay: u32,
    in_delay: bool,
    tick_frac: f64,
    samples_left: usize,
    /// How often the song has reached its end (or jumped back).
    pub loops: u32,
    /// Stereo separation 0 (mono) .. 1 (hard Amiga panning).
    pub separation: f32,
    /// Master gain of the mix.
    pub gain: f32,
}

impl Player {
    /// A player without a song. `const`, so it can initialise a static.
    ///
    /// # Returns
    /// A silent player (0 channels), separation 0.6, gain 0.5.
    pub const fn empty() -> Player {
        Player {
            m: 0 as *const u8,
            mod_len: 0,
            channels: 0,
            num_samples: 0,
            samples: [SAMPLE0; 32],
            song_len: 0,
            restart: 0,
            order: [0; 128],
            pat_base: 0,
            num_patterns: 0,
            sample_rate: 44100.0,
            ch: [CHANNEL_INIT; MAX_CH],
            speed: 6,
            tempo: 125,
            tick: 0,
            order_pos: 0,
            row: 0,
            break_row: -1,
            jump_order: -1,
            loop_jump: -1,
            pat_delay: 0,
            in_delay: false,
            tick_frac: 0.0,
            samples_left: 0,
            loops: 0,
            separation: 0.6,
            gain: 0.5,
        }
    }

    /// Parses a MOD file and resets playback to position 0. Separation and gain are kept. The
    /// buffer must stay valid and unchanged while the player is used.
    ///
    /// # Arguments
    /// * `m` - start of the MOD file in memory
    /// * `len` - its length in bytes
    /// * `sample_rate` - output sample rate in Hz, e.g. 44100
    ///
    /// # Returns
    /// The channel count (> 0), or -1 (file too short) / -2 (unsupported channel count).
    pub fn load(&mut self, m: *const u8, len: usize, sample_rate: f32) -> i32 {
        let sep = self.separation;
        let gain = self.gain;
        *self = Player::empty();
        self.separation = sep;
        self.gain = gain;
        if len < 600 {
            return -1;
        }
        self.m = m;
        self.mod_len = len;
        self.sample_rate = sample_rate as f64;
        let mut chn = if len >= 1084 {
            channels_from_tag(rd(m, 1080), rd(m, 1081), rd(m, 1082), rd(m, 1083))
        } else {
            0
        };
        let ns = if chn == 0 { 15 } else { 31 };
        if chn == 0 {
            chn = 4;
        }
        if chn > MAX_CH {
            return -2;
        }
        self.channels = chn;
        self.num_samples = ns;
        let so = 20 + ns * 30;
        self.read_orders(so);
        self.pat_base = so + 130 + if ns == 31 { 4 } else { 0 };
        self.read_samples(self.pat_base + self.num_patterns * ROWS * chn * 4);
        self.set_default_panning();
        chn as i32
    }

    /// Song length, restart position and order list (header at `so`).
    fn read_orders(&mut self, so: usize) {
        let m = self.m;
        let mut sl = rd(m, so) as usize;
        if sl < 1 {
            sl = 1;
        }
        if sl > 128 {
            sl = 128;
        }
        self.song_len = sl;
        let rs = rd(m, so + 1) as usize;
        self.restart = if rs < sl { rs } else { 0 };
        let mut maxp = 0;
        let mut i = 0;
        while i < 128 {
            let p = rd(m, so + 2 + i);
            self.order[i] = p;
            if p > maxp {
                maxp = p;
            }
            i += 1;
        }
        self.num_patterns = maxp as usize + 1;
    }

    /// Sample headers; the sample data follows the patterns, starting at `pos`.
    fn read_samples(&mut self, pos: usize) {
        let mut pos = pos;
        let mut s = 0;
        while s < self.num_samples {
            let o = 20 + s * 30;
            self.samples[s] = SampleInfo::read(self.m, o, pos, self.mod_len);
            pos += be16(self.m, o + 22) * 2;
            s += 1;
        }
    }

    /// Amiga panning: channels 1 and 4 left, 2 and 3 right, repeating.
    fn set_default_panning(&mut self) {
        let mut c = 0;
        while c < MAX_CH {
            let r = c % 4;
            self.ch[c].pan = if r == 0 || r == 3 { 0.0 } else { 1.0 };
            c += 1;
        }
    }

    /// Number of sample slots of the loaded MOD (15 or 31).
    ///
    /// # Returns
    /// The number of samples; sample `i`'s name is the 22 bytes at offset `20 + 30·i`.
    pub fn num_samples(&self) -> usize {
        self.num_samples
    }

    /// A byte of the loaded MOD file, e.g. for the song title.
    ///
    /// # Arguments
    /// * `i` - offset in the file
    ///
    /// # Returns
    /// The byte, or 0 outside the file.
    pub fn mod_byte(&self, i: usize) -> u32 {
        if i < self.mod_len {
            rd(self.m, i)
        } else {
            0
        }
    }

    /// Pattern number at a position of the order list.
    ///
    /// # Arguments
    /// * `pos` - song position
    ///
    /// # Returns
    /// The pattern number, or 0 outside the order list.
    pub fn order_at(&self, pos: usize) -> u32 {
        if pos < 128 {
            self.order[pos]
        } else {
            0
        }
    }

    /// Pattern number at the current song position.
    ///
    /// # Returns
    /// The pattern being played.
    pub fn pattern(&self) -> usize {
        self.order[self.order_pos] as usize
    }

    /// One cell of a pattern.
    ///
    /// # Arguments
    /// * `pattern` - pattern number
    /// * `row` - row, 0..63
    /// * `c` - channel, `0 .. channels`
    ///
    /// # Returns
    /// The cell, or an empty cell outside the song.
    pub fn cell(&self, pattern: usize, row: usize, c: usize) -> Cell {
        let o = self.pat_base + ((pattern * ROWS + row) * self.channels + c) * 4;
        if pattern >= self.num_patterns || o + 4 > self.mod_len {
            return Cell::EMPTY;
        }
        let m = self.m;
        Cell::decode(rd(m, o), rd(m, o + 1), rd(m, o + 2), rd(m, o + 3))
    }

    /// Jumps to a song position (row 0) and silences all channels.
    ///
    /// # Arguments
    /// * `order` - song position; out of range means 0
    pub fn set_position(&mut self, order: usize) {
        self.order_pos = if order < self.song_len { order } else { 0 };
        self.row = 0;
        self.tick = 0;
        self.samples_left = 0;
        self.break_row = -1;
        self.jump_order = -1;
        self.loop_jump = -1;
        self.pat_delay = 0;
        self.in_delay = false;
        let mut c = 0;
        while c < MAX_CH {
            self.ch[c].active = false;
            self.ch[c].gain = 0.0;
            c += 1;
        }
    }

    /// Renders the next frames of the song (silence if none is loaded).
    ///
    /// # Arguments
    /// * `out` - receives interleaved stereo, 2·`frames` floats
    /// * `chan_out` - receives one block of `frames` floats per channel (planar), channels·`frames` floats
    /// * `frames` - number of frames
    ///
    /// Both buffers must be large enough; they are written through raw pointers.
    pub fn render(&mut self, out: *mut f32, chan_out: *mut f32, frames: usize) {
        let nch = self.channels;
        fill(out, frames * 2, 0.0);
        fill(chan_out, nch * frames, 0.0);
        if nch == 0 {
            return;
        }
        let mut done = 0;
        while done < frames {
            if self.samples_left == 0 && !self.next_tick() {
                continue;
            }
            let n = umin(self.samples_left, frames - done);
            let mut c = 0;
            while c < nch {
                self.mix_channel(c, done, n, frames, out, chan_out);
                c += 1;
            }
            self.samples_left -= n;
            done += n;
        }
    }

    /// Processes one tick and computes how many output samples it lasts;
    /// false if that is zero (very high tempo at a low rate).
    fn next_tick(&mut self) -> bool {
        self.do_tick();
        self.tick_frac += self.sample_rate * 2.5 / self.tempo as f64;
        let n = self.tick_frac as usize;
        self.tick_frac -= n as f64;
        self.samples_left = n;
        n != 0
    }

    /// Mixing constants for channel `c`, or false if it plays nothing.
    fn mix_setup(&self, c: usize, out: &mut MixSetup) -> bool {
        let ch = &self.ch[c];
        let si = ch.sample;
        if si == 0 || si > self.num_samples || ch.out_period <= 0 {
            return false;
        }
        let s = &self.samples[si - 1];
        if s.len == 0 {
            return false;
        }
        let pan = 0.5 + (ch.pan - 0.5) * self.separation;
        *out = MixSetup {
            off: s.off,
            len: s.len,
            loop_start: s.loop_start,
            loop_len: s.loop_len,
            looped: s.loop_len > 2,
            loop_end: (s.loop_start + s.loop_len) as f64,
            step: PAL_CLOCK / (2.0 * ch.out_period as f64) / self.sample_rate,
            target: if ch.active {
                iclamp(ch.out_volume, 0, 64) as f32 / 64.0
            } else {
                0.0
            },
            gl: (1.0 - pan) * self.gain,
            gr: pan * self.gain,
        };
        true
    }

    /// Mixes `n` samples of channel `c` starting at frame `start`.
    fn mix_channel(
        &mut self,
        c: usize,
        start: usize,
        n: usize,
        frames: usize,
        out: *mut f32,
        chan_out: *mut f32,
    ) {
        let mut ms = MixSetup {
            off: 0,
            len: 0,
            loop_start: 0,
            loop_len: 0,
            looped: false,
            loop_end: 0.0,
            step: 0.0,
            target: 0.0,
            gl: 0.0,
            gr: 0.0,
        };
        if !self.mix_setup(c, &mut ms) {
            return;
        }
        let m = self.m;
        let ch = &mut self.ch[c];
        let base = c * frames + start;
        let mut i = 0;
        while i < n {
            ch.gain += (ms.target - ch.gain) * 0.02;
            if !ch.active && ch.gain < 0.0001 {
                ch.gain = 0.0;
                return;
            }
            let v = ms.sample_at(m, ch.pos) * (1.0 / 128.0) * ch.gain;
            wr(chan_out, base + i, v);
            add_to(out, (start + i) * 2, v * ms.gl);
            add_to(out, (start + i) * 2 + 1, v * ms.gr);
            if ch.active && !ms.advance(&mut ch.pos) {
                ch.active = false;
            }
            i += 1;
        }
    }

    // ---- sequencer

    /// One tick: the row on tick 0, the running effects on the others.
    fn do_tick(&mut self) {
        if self.tick == 0 {
            if self.in_delay {
                self.latch_all();
            } else {
                self.play_row();
            }
        } else {
            let mut c = 0;
            while c < self.channels {
                self.tick_effects(c);
                c += 1;
            }
        }
        self.tick += 1;
        if self.tick >= self.speed {
            self.tick = 0;
            self.end_row();
        }
    }

    fn latch_all(&mut self) {
        let mut c = 0;
        while c < self.channels {
            self.ch[c].latch();
            c += 1;
        }
    }

    /// Moves to the next row, honouring pattern delay, pattern loop, break and jump.
    fn end_row(&mut self) {
        if self.pat_delay > 0 {
            self.pat_delay -= 1;
            self.in_delay = true;
            return;
        }
        self.in_delay = false;
        if self.loop_jump >= 0 {
            self.row = self.loop_jump as usize;
            self.loop_jump = -1;
        } else if self.break_row >= 0 || self.jump_order >= 0 {
            self.take_break_or_jump();
        } else {
            self.row += 1;
            if self.row >= ROWS {
                let next = self.order_pos + 1;
                self.set_order(next, 0);
            }
        }
    }

    /// Bxx / Dxx: continue at another order position and/or row.
    fn take_break_or_jump(&mut self) {
        let next = if self.jump_order >= 0 {
            self.jump_order as usize
        } else {
            self.order_pos + 1
        };
        let mut row = if self.break_row >= 0 {
            self.break_row as usize
        } else {
            0
        };
        if row >= ROWS {
            row = 0;
        }
        self.jump_order = -1;
        self.break_row = -1;
        self.set_order(next, row);
    }

    /// Sets the order position (wrapping to the restart position at the end).
    fn set_order(&mut self, next: usize, row: usize) {
        if next >= self.song_len {
            self.loops += 1;
            self.order_pos = self.restart;
        } else {
            self.order_pos = next;
        }
        self.row = row;
        let mut c = 0;
        while c < MAX_CH {
            self.ch[c].loop_row = 0;
            self.ch[c].loop_count = 0;
            c += 1;
        }
    }

    /// Tick 0 of a row: read every channel's cell, start notes, apply row effects.
    fn play_row(&mut self) {
        let pat = self.pattern();
        let mut c = 0;
        while c < self.channels {
            let cell = self.cell(pat, self.row, c);
            self.ch[c].effect = cell.effect;
            self.ch[c].param = cell.param;
            if cell.is_note_delay() {
                self.ch[c].has_delayed = true;
                self.ch[c].delayed = cell.copy();
            } else {
                self.trigger(c, &cell);
            }
            if !self.global_effect(c, &cell) {
                self.ch[c].row_effect(&cell);
            }
            self.ch[c].latch();
            c += 1;
        }
    }

    /// Takes the sample (volume, finetune) and the note of `cell` for channel `c`.
    fn trigger(&mut self, c: usize, cell: &Cell) {
        let smp = cell.sample as usize;
        if smp > 0 && smp <= self.num_samples {
            let (v, ft) = (self.samples[smp - 1].volume, self.samples[smp - 1].finetune);
            let ch = &mut self.ch[c];
            ch.sample = smp;
            ch.volume = v;
            ch.finetune = ft;
        }
        if cell.period == 0 {
            return;
        }
        let ch_sample = self.ch[c].sample;
        let smp_len = if ch_sample > 0 {
            self.samples[ch_sample - 1].len
        } else {
            0
        };
        let ch = &mut self.ch[c];
        if cell.effect == 0xE && cell.x() == 5 {
            ch.finetune = cell.y();
        }
        let per = ft_period(nearest_note(cell.period as i32), ch.finetune);
        if cell.effect == 3 || cell.effect == 5 {
            ch.target = per; // tone portamento: slide there instead
            return;
        }
        ch.start_note(cell, per, smp_len);
    }

    /// Row effects that act on the sequencer (Bxx, Dxx, Fxx, E6x, EEx). Returns true
    /// if `cell` had one of them.
    fn global_effect(&mut self, c: usize, cell: &Cell) -> bool {
        let (e, x, y) = (cell.effect, cell.x(), cell.y());
        if e == 0xB {
            self.jump_order = cell.param as i32;
            if self.break_row < 0 {
                self.break_row = 0;
            }
        } else if e == 0xD {
            self.break_row = (x * 10 + y) as i32;
        } else if e == 0xF {
            self.set_speed_or_tempo(cell.param);
        } else if e == 0xE && x == 0x6 {
            self.pattern_loop(c, y);
        } else if e == 0xE && x == 0xE {
            if self.pat_delay == 0 && !self.in_delay {
                self.pat_delay = y;
            }
        } else {
            return false;
        }
        true
    }

    /// Fxx: speed for xx < 32, tempo otherwise (F00 is ignored).
    fn set_speed_or_tempo(&mut self, p: u32) {
        if p == 0 {
            return;
        }
        if p < 32 {
            self.speed = p;
        } else {
            self.tempo = p;
        }
    }

    /// E60 sets the loop start, E6x repeats from there x times.
    fn pattern_loop(&mut self, c: usize, y: u32) {
        let row = self.row;
        let ch = &mut self.ch[c];
        if y == 0 {
            ch.loop_row = row;
        } else if ch.loop_count == 0 {
            ch.loop_count = y;
            self.loop_jump = ch.loop_row as i32;
        } else {
            ch.loop_count -= 1;
            if ch.loop_count > 0 {
                self.loop_jump = ch.loop_row as i32;
            }
        }
    }

    /// Ticks after the first: delayed notes (EDx) here, everything else in the channel.
    fn tick_effects(&mut self, c: usize) {
        let ch = &self.ch[c];
        if ch.effect == 0xE && (ch.param >> 4) == 0xD {
            if self.tick == (ch.param & 15) && ch.has_delayed {
                let cell = self.ch[c].delayed.copy();
                self.ch[c].has_delayed = false;
                self.trigger(c, &cell);
            }
            self.ch[c].latch();
            return;
        }
        self.ch[c].tick_effect(self.tick);
    }
}

// ------------------------------------------------------------------ tests (native only)

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_mod() -> std::vec::Vec<u8> {
        let mut b = std::vec![0u8; 1084 + 1024 + 64];
        b[..4].copy_from_slice(b"test");
        let o = 20;
        b[o + 22..o + 24].copy_from_slice(&32u16.to_be_bytes());
        b[o + 25] = 64;
        b[o + 28..o + 30].copy_from_slice(&32u16.to_be_bytes());
        b[950] = 1;
        b[1080..1084].copy_from_slice(b"M.K.");
        b[1084] = 0x01;
        b[1085] = (428u16 & 0xFF) as u8;
        b[1086] = 0x10;
        for (i, v) in b[1084 + 1024..].iter_mut().enumerate() {
            *v = if i < 32 { 100 } else { 156 };
        }
        b
    }

    #[test]
    fn parses_and_plays() {
        let m = tiny_mod();
        let mut p = Player::empty();
        assert_eq!(p.load(m.as_ptr(), m.len(), 44100.0), 4);
        assert_eq!(p.samples[0].len, 64);
        assert_eq!(p.samples[0].loop_len, 64);
        let mut out = std::vec![0f32; 2 * 4096];
        let mut ch = std::vec![0f32; 4 * 4096];
        p.render(out.as_mut_ptr(), ch.as_mut_ptr(), 4096);
        assert!(out.iter().any(|v| v.abs() > 0.01));
        assert!(ch[..4096].iter().any(|v| v.abs() > 0.01));
    }

    #[test]
    fn random_patterns_do_not_panic() {
        let mut seed = 0x1234_5678u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..30 {
            let mut b = tiny_mod();
            for v in b[1084..1084 + 1024].iter_mut() {
                *v = (rnd() & 0xFF) as u8;
            }
            let mut p = Player::empty();
            assert!(p.load(b.as_ptr(), b.len(), 48000.0) > 0);
            let mut out = std::vec![0f32; 2 * 512];
            let mut ch = std::vec![0f32; 4 * 512];
            for _ in 0..400 {
                p.render(out.as_mut_ptr(), ch.as_mut_ptr(), 512);
                assert!(out.iter().all(|v| v.is_finite()));
            }
        }
    }

    #[test]
    fn truncated_files_are_safe() {
        let m = tiny_mod();
        for cut in [600usize, 900, 1083, 1084, 1500, 2100] {
            let mut p = Player::empty();
            p.load(m.as_ptr(), cut, 44100.0);
            let mut out = std::vec![0f32; 2 * 2048];
            let mut ch = std::vec![0f32; 32 * 2048];
            p.render(out.as_mut_ptr(), ch.as_mut_ptr(), 2048);
        }
    }

    #[test]
    fn notes() {
        assert_eq!(nearest_note(428), 12);
        assert_eq!(nearest_note(170), 28);
    }
}
