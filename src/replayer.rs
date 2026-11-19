// ProTracker MOD replayer.
//
// This file deliberately uses only the core language: `while` loops, arrays,
// structs, operators and casts. No iterators, no Option, no slices, no methods
// from `core` and no allocation. That way the very same source compiles as a
// normal Rust module (native tests, `cargo build --target wasm32-unknown-unknown`)
// and inside a minimal `#![no_core]` environment (see preview/minicore.rs).
//
// The MOD file is never copied or converted: patterns and samples are read
// straight from the caller's byte buffer (passed as a raw pointer + length).

pub const MAX_CH: usize = 32;
pub const ROWS: usize = 64;
const PAL_CLOCK: f64 = 7093789.2;
const PERIOD_MIN: i32 = 113;
const PERIOD_MAX: i32 = 856;

pub const BASE_PERIODS: [i32; 36] = [
    856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453, //
    428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226, //
    214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113,
];

// Periods per finetune nibble (0..7 = +0..+7, 8..15 = -8..-1), 36 notes each.
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

// ------------------------------------------------------------------ small helpers

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

fn imin(a: i32, b: i32) -> i32 {
    if a < b {
        a
    } else {
        b
    }
}
fn imax(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}
fn iclamp(v: i32, lo: i32, hi: i32) -> i32 {
    imax(lo, imin(v, hi))
}
fn umin(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}
fn iabs(v: i32) -> i32 {
    if v < 0 {
        0 - v
    } else {
        v
    }
}
fn is_digit(c: u32) -> bool {
    c >= 48 && c <= 57
}

fn ft_period(note: usize, ft_nibble: u32) -> i32 {
    let n = if note > 35 { 35 } else { note };
    FT_PERIODS[(ft_nibble & 15) as usize * 36 + n]
}

/// Index (0 = C-1 .. 35 = B-3) of the note closest to `period` (finetune 0).
pub fn nearest_note(period: i32) -> usize {
    let mut best = 0;
    let mut diff = 1 << 30;
    let mut i = 0;
    while i < 36 {
        let d = iabs(BASE_PERIODS[i] - period);
        if d < diff {
            diff = d;
            best = i;
        }
        i += 1;
    }
    best
}

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

// ------------------------------------------------------------------ state

pub struct Cell {
    pub sample: u32,
    pub period: u32,
    pub effect: u32,
    pub param: u32,
}

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
    d_sample: u32,
    d_period: u32,
    d_effect: u32,
    d_param: u32,
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
    d_sample: 0,
    d_period: 0,
    d_effect: 0,
    d_param: 0,
};

pub struct Player {
    m: *const u8,
    mod_len: usize,
    pub channels: usize,
    num_samples: usize,
    smp_off: [usize; 32],
    smp_len: [usize; 32],
    smp_ls: [usize; 32],
    smp_ll: [usize; 32],
    smp_vol: [i32; 32],
    smp_ft: [u32; 32],
    pub song_len: usize,
    restart: usize,
    order: [u32; 128],
    pat_base: usize,
    num_patterns: usize,
    sample_rate: f64,
    ch: [Channel; MAX_CH],
    pub speed: u32,
    pub tempo: u32,
    tick: u32,
    pub order_pos: usize,
    pub row: usize,
    break_row: i32, // -1 = none
    jump_order: i32,
    loop_jump: i32,
    pat_delay: u32,
    in_delay: bool,
    tick_frac: f64,
    samples_left: usize,
    pub loops: u32,
    pub separation: f32,
    pub gain: f32,
}

impl Player {
    pub const fn empty() -> Player {
        Player {
            m: 0 as *const u8,
            mod_len: 0,
            channels: 0,
            num_samples: 0,
            smp_off: [0; 32],
            smp_len: [0; 32],
            smp_ls: [0; 32],
            smp_ll: [0; 32],
            smp_vol: [0; 32],
            smp_ft: [0; 32],
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

    /// Parse the MOD at `m` (`len` bytes). The buffer must stay valid and
    /// unchanged while the player is used. Returns the channel count (> 0)
    /// or -1 (too short) / -2 (unsupported).
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

        let mut ns = 31;
        let mut chn = 4;
        if len >= 1084 {
            let (a, b, c, d) = (rd(m, 1080), rd(m, 1081), rd(m, 1082), rd(m, 1083));
            let tag = (a << 24) | (b << 16) | (c << 8) | d;
            // 'M.K.' 'M!K!' 'M&K!' 'N.T.' 'FLT4' '4CHN' / '6CHN' / '8CHN' 'FLT8' 'OCTA' 'CD81'
            if tag == 0x4D2E4B2E
                || tag == 0x4D214B21
                || tag == 0x4D264B21
                || tag == 0x4E2E542E
                || tag == 0x464C5434
                || tag == 0x3443484E
            {
                chn = 4;
            } else if tag == 0x3643484E {
                chn = 6;
            } else if tag == 0x3843484E
                || tag == 0x464C5438
                || tag == 0x4F435441
                || tag == 0x43443831
            {
                chn = 8;
            } else if is_digit(a) && is_digit(b) && c == 67 && d == 72 {
                chn = ((a - 48) * 10 + (b - 48)) as usize; // xxCH
            } else if is_digit(a) && b == 67 && c == 72 && d == 78 {
                chn = (a - 48) as usize; // xCHN
            } else {
                ns = 15;
            }
        } else {
            ns = 15;
        }
        if chn == 0 || chn > MAX_CH {
            return -2;
        }
        self.channels = chn;
        self.num_samples = ns;

        let so = 20 + ns * 30;
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
        self.pat_base = so + 130 + if ns == 31 { 4 } else { 0 };

        let mut pos = self.pat_base + self.num_patterns * ROWS * chn * 4;
        let mut s = 0;
        while s < ns {
            let o = 20 + s * 30;
            let slen = be16(m, o + 22) * 2;
            let start = pos;
            let mut avail = 0;
            if start < len {
                avail = umin(slen, len - start);
            }
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
            self.smp_off[s] = start;
            self.smp_len[s] = avail;
            self.smp_ls[s] = ls;
            self.smp_ll[s] = ll;
            let v = rd(m, o + 25) as i32;
            self.smp_vol[s] = if v > 64 { 64 } else { v };
            self.smp_ft[s] = rd(m, o + 24) & 15;
            pos += slen;
            s += 1;
        }

        let mut c = 0;
        while c < MAX_CH {
            let r = c % 4;
            self.ch[c].pan = if r == 0 || r == 3 { 0.0 } else { 1.0 };
            c += 1;
        }
        chn as i32
    }

    pub fn pattern(&self) -> usize {
        self.order[self.order_pos] as usize
    }

    pub fn cell(&self, pattern: usize, row: usize, c: usize) -> Cell {
        let o = self.pat_base + ((pattern * ROWS + row) * self.channels + c) * 4;
        if pattern >= self.num_patterns || o + 4 > self.mod_len {
            return Cell {
                sample: 0,
                period: 0,
                effect: 0,
                param: 0,
            };
        }
        let m = self.m;
        let (b0, b1, b2, b3) = (rd(m, o), rd(m, o + 1), rd(m, o + 2), rd(m, o + 3));
        Cell {
            sample: (b0 & 0xF0) | (b2 >> 4),
            period: ((b0 & 15) << 8) | b1,
            effect: b2 & 15,
            param: b3,
        }
    }

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

    /// Render `frames` frames: interleaved stereo into `out` (2*frames floats)
    /// and planar per-channel signals into `chan_out` (channels*frames floats).
    pub fn render(&mut self, out: *mut f32, chan_out: *mut f32, frames: usize) {
        let nch = self.channels;
        let mut i = 0;
        while i < frames * 2 {
            wr(out, i, 0.0);
            i += 1;
        }
        i = 0;
        while i < nch * frames {
            wr(chan_out, i, 0.0);
            i += 1;
        }
        if nch == 0 {
            return;
        }
        let mut done = 0;
        while done < frames {
            if self.samples_left == 0 {
                self.do_tick();
                self.tick_frac += self.sample_rate * 2.5 / self.tempo as f64;
                let n = self.tick_frac as usize;
                self.tick_frac -= n as f64;
                self.samples_left = n;
                if n == 0 {
                    continue;
                }
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

    fn mix_channel(
        &mut self,
        c: usize,
        start: usize,
        n: usize,
        frames: usize,
        out: *mut f32,
        chan_out: *mut f32,
    ) {
        let sep = self.separation;
        let master = self.gain;
        let sr = self.sample_rate;
        let m = self.m;
        let si = self.ch[c].sample;
        if si == 0 || si > self.num_samples || self.ch[c].out_period <= 0 {
            return;
        }
        let s = si - 1;
        let off = self.smp_off[s];
        let len = self.smp_len[s];
        let ls = self.smp_ls[s];
        let ll = self.smp_ll[s];
        if len == 0 {
            return;
        }
        let looped = ll > 2;
        let loop_end = (ls + ll) as f64;
        let ch = &mut self.ch[c];
        let target = if ch.active {
            iclamp(ch.out_volume, 0, 64) as f32 / 64.0
        } else {
            0.0
        };
        let pan = 0.5 + (ch.pan - 0.5) * sep;
        let gl = (1.0 - pan) * master;
        let gr = pan * master;
        let step = PAL_CLOCK / (2.0 * ch.out_period as f64) / sr;
        let base = c * frames + start;
        let mut i = 0;
        while i < n {
            ch.gain += (target - ch.gain) * 0.02;
            if !ch.active && ch.gain < 0.0001 {
                ch.gain = 0.0;
                return;
            }
            let ip = ch.pos as usize;
            let frac = (ch.pos - ip as f64) as f32;
            let a = if ip < len { rd_i8(m, off + ip) } else { 0.0 };
            let nx = ip + 1;
            let b = if looped && nx as f64 >= loop_end {
                rd_i8(m, off + ls)
            } else if nx < len {
                rd_i8(m, off + nx)
            } else {
                0.0
            };
            let v = (a + (b - a) * frac) * (1.0 / 128.0) * ch.gain;
            wr(chan_out, base + i, v);
            add_to(out, (start + i) * 2, v * gl);
            add_to(out, (start + i) * 2 + 1, v * gr);
            if ch.active {
                ch.pos += step;
                if looped {
                    while ch.pos >= loop_end {
                        ch.pos -= ll as f64;
                    }
                } else if ch.pos >= len as f64 {
                    ch.active = false;
                }
            }
            i += 1;
        }
    }

    fn do_tick(&mut self) {
        if self.tick == 0 {
            if self.in_delay {
                let mut c = 0;
                while c < self.channels {
                    self.ch[c].out_period = self.ch[c].period;
                    self.ch[c].out_volume = self.ch[c].volume;
                    c += 1;
                }
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
        } else {
            self.row += 1;
            if self.row >= ROWS {
                let next = self.order_pos + 1;
                self.set_order(next, 0);
            }
        }
    }

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

    fn play_row(&mut self) {
        let pat = self.pattern();
        let mut c = 0;
        while c < self.channels {
            let cell = self.cell(pat, self.row, c);
            self.ch[c].effect = cell.effect;
            self.ch[c].param = cell.param;
            let is_delay = cell.effect == 0xE && (cell.param >> 4) == 0xD && (cell.param & 15) > 0;
            if is_delay {
                let ch = &mut self.ch[c];
                ch.has_delayed = true;
                ch.d_sample = cell.sample;
                ch.d_period = cell.period;
                ch.d_effect = cell.effect;
                ch.d_param = cell.param;
            } else {
                self.trigger(c, &cell);
            }
            self.row_effect(c, &cell);
            let ch = &mut self.ch[c];
            ch.out_period = ch.period;
            ch.out_volume = ch.volume;
            c += 1;
        }
    }

    fn trigger(&mut self, c: usize, cell: &Cell) {
        let smp = cell.sample as usize;
        if smp > 0 && smp <= self.num_samples {
            let v = self.smp_vol[smp - 1];
            let ft = self.smp_ft[smp - 1];
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
            self.smp_len[ch_sample - 1]
        } else {
            0
        };
        let ch = &mut self.ch[c];
        if cell.effect == 0xE && (cell.param >> 4) == 5 {
            ch.finetune = cell.param & 15;
        }
        let idx = nearest_note(cell.period as i32);
        let per = ft_period(idx, ch.finetune);
        if cell.effect == 3 || cell.effect == 5 {
            ch.target = per;
            return;
        }
        ch.period = per;
        ch.pos = 0.0;
        ch.active = ch.sample > 0;
        if ch.vib_wave < 4 {
            ch.vib_pos = 0;
        }
        if ch.trem_wave < 4 {
            ch.trem_pos = 0;
        }
        if cell.effect == 9 {
            if cell.param > 0 {
                ch.offset_mem = cell.param;
            }
            ch.pos = (ch.offset_mem * 256) as f64;
            if ch.pos >= smp_len as f64 {
                ch.active = false;
            }
        }
    }

    fn row_effect(&mut self, c: usize, cell: &Cell) {
        let x = cell.param >> 4;
        let y = cell.param & 15;
        let row = self.row;
        let e = cell.effect;
        if e == 0xB {
            self.jump_order = cell.param as i32;
            if self.break_row < 0 {
                self.break_row = 0;
            }
            return;
        }
        if e == 0xD {
            self.break_row = (x * 10 + y) as i32;
            return;
        }
        if e == 0xF {
            if cell.param == 0 {
            } else if cell.param < 32 {
                self.speed = cell.param;
            } else {
                self.tempo = cell.param;
            }
            return;
        }
        if e == 0xE && x == 0x6 {
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
            return;
        }
        if e == 0xE && x == 0xE {
            if self.pat_delay == 0 && !self.in_delay {
                self.pat_delay = y;
            }
            return;
        }
        let ch = &mut self.ch[c];
        if e == 0x3 {
            if cell.param > 0 {
                ch.porta_speed = cell.param as i32;
            }
        } else if e == 0x4 {
            if x > 0 {
                ch.vib_speed = x;
            }
            if y > 0 {
                ch.vib_depth = y;
            }
        } else if e == 0x7 {
            if x > 0 {
                ch.trem_speed = x;
            }
            if y > 0 {
                ch.trem_depth = y;
            }
        } else if e == 0x8 {
            ch.pan = cell.param as f32 / 255.0;
        } else if e == 0xC {
            ch.volume = imin(cell.param as i32, 64);
        } else if e == 0xE {
            if x == 0x1 {
                ch.period = imax(ch.period - y as i32, PERIOD_MIN);
            } else if x == 0x2 {
                ch.period = imin(ch.period + y as i32, PERIOD_MAX);
            } else if x == 0x4 {
                ch.vib_wave = y;
            } else if x == 0x7 {
                ch.trem_wave = y;
            } else if x == 0xA {
                ch.volume = imin(ch.volume + y as i32, 64);
            } else if x == 0xB {
                ch.volume = imax(ch.volume - y as i32, 0);
            } else if x == 0xC && y == 0 {
                ch.volume = 0;
            }
        }
    }

    fn tick_effects(&mut self, c: usize) {
        let tick = self.tick;
        let e = self.ch[c].effect;
        let p = self.ch[c].param;
        let x = p >> 4;
        let y = p & 15;

        if e == 0xE && x == 0xD {
            if tick == y && self.ch[c].has_delayed {
                let cell = Cell {
                    sample: self.ch[c].d_sample,
                    period: self.ch[c].d_period,
                    effect: self.ch[c].d_effect,
                    param: self.ch[c].d_param,
                };
                self.ch[c].has_delayed = false;
                self.trigger(c, &cell);
            }
            let ch = &mut self.ch[c];
            ch.out_period = ch.period;
            ch.out_volume = ch.volume;
            return;
        }

        let ch = &mut self.ch[c];
        ch.out_period = ch.period;
        let mut vol_slide = false;
        if e == 0x0 {
            if p != 0 {
                let t = tick % 3;
                let add = if t == 1 {
                    x as usize
                } else if t == 2 {
                    y as usize
                } else {
                    0
                };
                let base = nearest_note_ft(ch.period, ch.finetune);
                ch.out_period = ft_period(base + add, ch.finetune);
            }
        } else if e == 0x1 {
            ch.period = imax(ch.period - p as i32, PERIOD_MIN);
            ch.out_period = ch.period;
        } else if e == 0x2 {
            ch.period = imin(ch.period + p as i32, PERIOD_MAX);
            ch.out_period = ch.period;
        } else if e == 0x3 || e == 0x5 {
            if ch.target != 0 {
                if ch.period < ch.target {
                    ch.period = imin(ch.period + ch.porta_speed, ch.target);
                } else if ch.period > ch.target {
                    ch.period = imax(ch.period - ch.porta_speed, ch.target);
                }
            }
            ch.out_period = ch.period;
            vol_slide = e == 0x5;
        } else if e == 0x4 || e == 0x6 {
            let d = wave_value(ch.vib_wave, ch.vib_pos) * ch.vib_depth as i32 / 128;
            ch.out_period = imax(ch.period + d, 1);
            ch.vib_pos = (ch.vib_pos + ch.vib_speed) & 63;
            vol_slide = e == 0x6;
        } else if e == 0x7 {
            let d = wave_value(ch.trem_wave, ch.trem_pos) * ch.trem_depth as i32 / 64;
            ch.out_volume = iclamp(ch.volume + d, 0, 64);
            ch.trem_pos = (ch.trem_pos + ch.trem_speed) & 63;
            return;
        } else if e == 0xA {
            vol_slide = true;
        } else if e == 0xE {
            if x == 0x9 {
                if y > 0 && tick % y == 0 {
                    ch.pos = 0.0;
                    ch.active = ch.sample > 0;
                }
            } else if x == 0xC {
                if tick == y {
                    ch.volume = 0;
                }
            }
        }
        if vol_slide {
            if x > 0 {
                ch.volume = imin(ch.volume + x as i32, 64);
            } else {
                ch.volume = imax(ch.volume - y as i32, 0);
            }
        }
        ch.out_volume = ch.volume;
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
        for i in 0..64 {
            b[1084 + 1024 + i] = if i < 32 { 100 } else { 156 };
        }
        b
    }

    #[test]
    fn parses_and_plays() {
        let m = tiny_mod();
        let mut p = Player::empty();
        assert_eq!(p.load(m.as_ptr(), m.len(), 44100.0), 4);
        assert_eq!(p.smp_len[0], 64);
        assert_eq!(p.smp_ll[0], 64);
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
            for i in 0..1024 {
                b[1084 + i] = (rnd() & 0xFF) as u8;
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
