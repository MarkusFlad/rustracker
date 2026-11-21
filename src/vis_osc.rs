//! Oscilloscope view of the framebuffer renderer: master L/R, XY and one trace per channel.

use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::vis::*;
use crate::vis_ui::cell_text;

/// Where the parts of the oscilloscope view go.
struct OscLayout {
    pad: i32,
    top_h: i32,
    master_w: i32,
    xy_w: i32,
    nch: i32,
    cols: i32,
    cell_w: i32,
    cell_h: i32,
}

impl OscLayout {
    fn new(s: &Surface, fs: i32, nch: i32) -> OscLayout {
        let pad = 3 * fs;
        let top_h = s.h * 42 / 100;
        let xy_w = imin(top_h, s.w / 3);
        let nch = if nch < 1 { 4 } else { nch };
        let cols = if nch <= 4 { 2 } else { 4 };
        let rows = (nch + cols - 1) / cols;
        OscLayout {
            pad,
            top_h,
            master_w: s.w - xy_w - pad * 3,
            xy_w,
            nch,
            cols,
            cell_w: (s.w - pad * (cols + 1)) / cols,
            cell_h: (s.h - top_h - pad * (rows + 1)) / rows,
        }
    }
    fn master(&self, s: &Surface) -> Surface {
        s.sub(self.pad, self.pad, self.master_w, self.top_h - self.pad)
    }
    fn xy(&self, s: &Surface) -> Surface {
        s.sub(
            self.pad * 2 + self.master_w,
            self.pad,
            self.xy_w,
            self.top_h - self.pad,
        )
    }
    fn channel_xy(&self, c: i32) -> (i32, i32) {
        (
            self.pad + (c % self.cols) * (self.cell_w + self.pad),
            self.top_h + self.pad + (c / self.cols) * (self.cell_h + self.pad),
        )
    }
    fn channel(&self, s: &Surface, c: i32) -> Surface {
        let (x, y) = self.channel_xy(c);
        s.sub(x, y, self.cell_w, self.cell_h)
    }
}

impl Vis {
    /// First rising zero crossing of a scope signal in slot `k` starting at `off`, so
    /// that periodic waves stand still; `off` if there is none within the window.
    pub(crate) fn trigger(&self, k: usize, off: usize, win: usize) -> usize {
        let mut i = off + 1;
        while i < off + SCOPE_LEN - win {
            if self.sv(k, i - 1) <= 0.0 && self.sv(k, i) > 0.0 {
                return i;
            }
            i += 1;
        }
        off
    }

    /// Draws `win` scope values of slot `k` from `start` across the whole surface as a
    /// glowing trace in colour `c`, amplified by `gain`.
    pub(crate) fn trace(&self, s: &Surface, k: usize, start: usize, win: usize, c: u32, gain: f32) {
        let th = imax(self.fs - 1, 1);
        let (w, h) = (s.w as f32, s.h as f32);
        let mut px = 0.0;
        let mut py = 0.0;
        let mut i = 0;
        while i < win {
            let v = clamp1(self.sv(k, start + i) * gain);
            let x = i as f32 / (win - 1) as f32 * (w - 1.0);
            let y = h * 0.5 - v * h * 0.44;
            if i > 0 {
                s.trace_line(px, py, x, y, c, th);
            }
            px = x;
            py = y;
            i += 1;
        }
    }

    /// The oscilloscope view: master L/R (green/amber), XY figure and one trace per
    /// channel with its current pattern cell.
    pub(crate) fn draw_osc(&mut self, s: &Surface, cur: i32, pl: &Player) {
        let nch = if cur >= 0 {
            self.msgs[cur as usize].nch as i32
        } else {
            pl.channels as i32
        };
        let lay = OscLayout::new(s, self.fs, nch);
        self.draw_osc_frames(s, &lay);
        if cur < 0 {
            return;
        }
        let k = cur as usize;
        self.draw_master(&lay.master(s), k);
        self.draw_xy(&lay.xy(s), k);
        let mut c = 0;
        while c < lay.nch {
            self.draw_channel(&lay.channel(s, c), k, c, pl);
            c += 1;
        }
    }

    /// Grids and titles of all oscilloscope parts.
    fn draw_osc_frames(&self, s: &Surface, lay: &OscLayout) {
        let fs = self.fs;
        let (pad, top) = (lay.pad, lay.top_h - lay.pad);
        s.grid(pad, pad, lay.master_w, top);
        s.grid(pad * 2 + lay.master_w, pad, lay.xy_w, top);
        let mut c = 0;
        while c < lay.nch {
            let (x, y) = lay.channel_xy(c);
            s.grid(x, y, lay.cell_w, lay.cell_h);
            c += 1;
        }
        s.text(pad + 3 * fs, pad + 3 * fs, b"MASTER L/R", fs, PHOSPHOR);
        s.text(
            pad * 2 + lay.master_w + 3 * fs,
            pad + 3 * fs,
            b"XY",
            fs,
            PHOSPHOR,
        );
    }

    /// Master left (green) and right (amber), triggered on the left channel.
    fn draw_master(&self, s: &Surface, k: usize) {
        let ml = self.msgs[k].nch as usize * SCOPE_LEN;
        let win = 768;
        let st = self.trigger(k, ml, win) - ml;
        self.trace(s, k, ml + SCOPE_LEN + st, win, AMBER, 1.6);
        self.trace(s, k, ml + st, win, PHOSPHOR, 1.6);
    }

    /// Lissajous figure: x = left, y = right.
    pub(crate) fn draw_xy_figure(&self, s: &Surface, k: usize, gain: f32, alpha: u32) {
        let ml = self.msgs[k].nch as usize * SCOPE_LEN;
        let mr = ml + SCOPE_LEN;
        let cx = s.w as f32 / 2.0;
        let cy = s.h as f32 / 2.0;
        let r = imin(s.w, s.h) as f32 * 0.45;
        let mut i = 1;
        while i < SCOPE_LEN {
            let l0 = clamp1(self.sv(k, ml + i - 1) * gain);
            let r0 = clamp1(self.sv(k, mr + i - 1) * gain);
            let l1 = clamp1(self.sv(k, ml + i) * gain);
            let r1 = clamp1(self.sv(k, mr + i) * gain);
            s.line(
                cx + l0 * r,
                cy - r0 * r,
                cx + l1 * r,
                cy - r1 * r,
                PHOSPHOR,
                alpha,
            );
            i += 1;
        }
    }

    fn draw_xy(&self, s: &Surface, k: usize) {
        self.draw_xy_figure(s, k, 1.6, 150);
    }

    /// Trace and label (number and current cell) of channel `c`.
    fn draw_channel(&self, s: &Surface, k: usize, c: i32, pl: &Player) {
        let fs = self.fs;
        let m = &self.msgs[k];
        let cwin = 512;
        let st = self.trigger(k, c as usize * SCOPE_LEN, cwin);
        self.trace(s, k, st, cwin, PHOSPHOR, 1.2);
        let cell = pl.cell(m.pattern as usize, m.row as usize, c as usize);
        let mut t = TextBuf::new();
        t.s(b"KANAL ");
        t.num(c as u32 + 1, 1, 48);
        t.s(b"  ");
        cell_text(&mut t, &cell, 2);
        s.text_buf(3 * fs, 3 * fs, &t, fs, PHOSPHOR);
    }
}
