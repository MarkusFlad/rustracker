//! User interface of the framebuffer renderer: the button bar, pointer and keyboard input,
//! and the panel with song info, order list and pattern rows.

use crate::gfx::*;
use crate::math::{imax, imin};
use crate::replayer::Player;
use crate::vis::*;
use crate::vis_bench::*;

impl Vis {
    // ------------------------------------------------------------------ input

    /// A click or tap. View, 3D and palette buttons are handled here.
    ///
    /// # Arguments
    /// * `x` - column in framebuffer pixels of the last frame
    /// * `y` - row in framebuffer pixels
    ///
    /// # Returns
    /// The action for JS: one of the `ACT_*` constants (argument in [`Vis::action_arg`]);
    /// `ACT_NONE` if nothing is to be done.
    pub fn pointer(&mut self, x: i32, y: i32) -> i32 {
        if self.bn_state != BN_OFF {
            // a click closes the benchmark result; while it runs, clicks are ignored
            if self.bn_state == BN_DONE {
                self.bench_stop();
            }
            return ACT_NONE;
        }
        let i = self.button_at(x, y);
        if i >= 0 {
            return self.button(self.btn_id[i as usize]);
        }
        let idx = self.order_at(x, y);
        if idx >= 0 {
            self.action_arg = idx;
            return ACT_SEEK;
        }
        ACT_NONE
    }

    /// Index of the button under (x, y), or -1.
    fn button_at(&self, x: i32, y: i32) -> i32 {
        let mut i = 0;
        while i < self.nbtn {
            let inside_x = x >= self.btn_x[i] && x < self.btn_x[i] + self.btn_w[i];
            if inside_x && y >= self.btn_y[i] && y < self.btn_y[i] + self.btn_h[i] {
                return i as i32;
            }
            i += 1;
        }
        -1
    }

    /// Song position of the order list entry under (x, y), or -1.
    fn order_at(&self, x: i32, y: i32) -> i32 {
        if self.ord_n <= 0 || x < self.ord_x || y < self.ord_y {
            return -1;
        }
        let col = (x - self.ord_x) / self.ord_bw;
        let row = (y - self.ord_y) / self.ord_bh;
        let idx = row * self.ord_cols + col;
        if col < self.ord_cols && idx >= 0 && idx < self.ord_n {
            idx
        } else {
            -1
        }
    }

    /// Carries out button `id` (see `pointer`).
    pub(crate) fn button(&mut self, id: u32) -> i32 {
        if id == B_PLAY {
            return ACT_PLAY;
        }
        if id == B_STOP {
            return ACT_STOP;
        }
        if id == B_OPEN {
            return ACT_OPEN;
        }
        if id == B_VIEW {
            self.view = self.next_view(self.view);
            self.f_valid = false;
            return ACT_NONE;
        }
        if id == B_3D {
            self.spec3d = !self.spec3d;
            return ACT_NONE;
        }
        if id == B_PAL {
            self.pal_idx = (self.pal_idx + 1) % 3;
            self.make_palette();
            return ACT_NONE;
        }
        if id == B_LOOP {
            self.looping = !self.looping;
            self.action_arg = if self.looping { 1 } else { 0 };
            return ACT_LOOP;
        }
        if id == B_RES {
            self.vx_fine = !self.vx_fine;
            return ACT_NONE;
        }
        if id == B_FULL {
            self.fullscreen = !self.fullscreen;
            self.action_arg = if self.fullscreen { 1 } else { 0 };
            return ACT_FULLSCREEN;
        }
        ACT_NONE
    }

    /// A key press: space play/pause, s stop, o open, v view, 3 2D/3D, p palette, l loop,
    /// f fullscreen, r voxel resolution, b benchmark (upper or lower case). While the
    /// benchmark runs only b (cancel) works; its result closes with any key.
    ///
    /// # Arguments
    /// * `code` - character code of the key
    ///
    /// # Returns
    /// The action for JS: one of the `ACT_*` constants (argument in [`Vis::action_arg`]);
    /// `ACT_NONE` if nothing is to be done.
    pub fn key(&mut self, code: u32) -> i32 {
        if self.bn_state == BN_DONE {
            self.bench_stop(); // any key closes the result
            return ACT_NONE;
        }
        if code == 66 || code == 98 {
            if self.bn_state == BN_RUN {
                self.bench_stop();
                return ACT_NONE;
            }
            self.bench_start(BENCH_SECS);
            return ACT_BENCH;
        }
        if self.bn_state == BN_RUN {
            return ACT_NONE; // the benchmark chooses the views
        }
        if code == 32 {
            return ACT_PLAY;
        }
        if code == 83 || code == 115 {
            return ACT_STOP;
        }
        if code == 79 || code == 111 {
            return ACT_OPEN;
        }
        if code == 86 || code == 118 {
            return self.button(B_VIEW);
        }
        if code == 51 {
            return self.button(B_3D);
        }
        if code == 80 || code == 112 {
            return self.button(B_PAL);
        }
        if code == 76 || code == 108 {
            return self.button(B_LOOP);
        }
        if code == 70 || code == 102 {
            return self.button(B_FULL);
        }
        if code == 82 || code == 114 {
            return self.button(B_RES);
        }
        ACT_NONE
    }
    /// JS reports the browser's fullscreen state (e.g. after Escape).
    ///
    /// # Arguments
    /// * `on` - `true` in fullscreen
    pub fn set_fullscreen(&mut self, on: bool) {
        self.fullscreen = on;
    }

    // ------------------------------------------------------------------ top bar

    /// Draws a button at (x, y), wrapping to a new bar row if it does not fit, and
    /// registers it for hit tests. Returns the x position of the next button.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_button<const N: usize>(
        &mut self,
        s: &Surface,
        x: i32,
        y: i32,
        h: i32,
        label: &[u8; N],
        id: u32,
        on: bool,
    ) -> i32 {
        let fs = self.fs;
        let w = (N as i32 + 2) * 6 * fs;
        let (x, y) = if x + w > self.w && x > 3 * fs {
            // wrap to a new row of the bar
            let row_h = h + 4 * fs;
            self.bar_rows += 1;
            s.fill(0, y + h + 2 * fs, self.w, row_h, PANEL);
            (3 * fs, y + row_h)
        } else {
            (x, y)
        };
        self.bar_y = y;
        s.fill(x, y, w, h, if on { PHOS_DIM } else { rgb(18, 32, 26) });
        s.rect(x, y, w, h, PHOSPHOR, if on { 200 } else { 90 });
        s.text(
            x + 6 * fs,
            y + (h - 7 * fs) / 2,
            label,
            fs,
            if on { WHITE } else { PHOSPHOR },
        );
        if self.nbtn < 12 {
            let i = self.nbtn;
            self.btn_x[i] = x;
            self.btn_y[i] = y;
            self.btn_w[i] = w;
            self.btn_h[i] = h;
            self.btn_id[i] = id;
            self.nbtn += 1;
        }
        x + w + 3 * fs
    }

    /// Draws the button bar (wrapping onto more rows when narrow); returns its height.
    pub(crate) fn draw_bar(&mut self, s: &Surface, bar_h: i32) -> i32 {
        let fs = self.fs;
        s.fill(0, 0, self.w, bar_h, PANEL);
        self.nbtn = 0;
        self.bar_rows = 1;
        self.bar_y = 2 * fs;
        let h = bar_h - 4 * fs;
        let narrow = self.w < 76 * 6 * fs;
        let mut x = 3 * fs;
        if self.playing {
            x = self.add_button(s, x, self.bar_y, h, b"\x81", B_PLAY, true);
        } else {
            x = self.add_button(s, x, self.bar_y, h, b"\x80", B_PLAY, false);
        }
        x = self.add_button(s, x, self.bar_y, h, b"\x82", B_STOP, false);
        x = if narrow {
            self.add_button(s, x, self.bar_y, h, b"FILE", B_OPEN, false)
        } else {
            self.add_button(s, x, self.bar_y, h, b"FILE ...", B_OPEN, false)
        };
        x = self.add_view_button(s, x, h, narrow);
        if self.view == V_SPEC {
            let on = self.spec3d;
            x = self.add_button(s, x, self.bar_y, h, b"3D", B_3D, on);
        }
        if self.view == V_MANDEL || self.view == V_CUBE || self.view == V_VOXEL {
            x = self.add_button(s, x, self.bar_y, h, b"COLOURS", B_PAL, false);
        }
        if self.view == V_VOXEL || self.view == V_LAKE {
            let on = self.vx_fine;
            x = if on {
                self.add_button(s, x, self.bar_y, h, b"FINE", B_RES, true)
            } else {
                self.add_button(s, x, self.bar_y, h, b"COARSE", B_RES, false)
            };
        }
        let on = self.looping;
        x = self.add_button(s, x, self.bar_y, h, b"LOOP", B_LOOP, on);
        let on = self.fullscreen;
        self.add_button(s, x, self.bar_y, h, b"FULL", B_FULL, on);
        let total = bar_h * self.bar_rows;
        s.hline(0, self.w - 1, total - 1, LINE, 256);
        total
    }

    /// The view switch button, labelled with the current view (short labels if narrow).
    fn add_view_button(&mut self, s: &Surface, x: i32, h: i32, narrow: bool) -> i32 {
        let y = self.bar_y;
        let v = self.view;
        if narrow {
            if v == V_OSC {
                self.add_button(s, x, y, h, b"SCOPE", B_VIEW, false)
            } else if v == V_MANDEL {
                self.add_button(s, x, y, h, b"MANDEL", B_VIEW, false)
            } else if v == V_SPEC {
                self.add_button(s, x, y, h, b"SPECTR", B_VIEW, false)
            } else if v == V_VOXEL {
                self.add_button(s, x, y, h, b"VOXEL", B_VIEW, false)
            } else if v == V_LAKE {
                self.add_button(s, x, y, h, b"LAKE", B_VIEW, false)
            } else if v == V_DEMO {
                self.add_button(s, x, y, h, b"DEMO", B_VIEW, false)
            } else {
                self.add_button(s, x, y, h, b"CUBE", B_VIEW, false)
            }
        } else if v == V_OSC {
            self.add_button(s, x, y, h, b"VIEW: OSCILLOSCOPE", B_VIEW, false)
        } else if v == V_MANDEL {
            self.add_button(s, x, y, h, b"VIEW: MANDELBROT", B_VIEW, false)
        } else if v == V_SPEC {
            self.add_button(s, x, y, h, b"VIEW: SPECTRUM", B_VIEW, false)
        } else if v == V_VOXEL {
            self.add_button(s, x, y, h, b"VIEW: VOXEL", B_VIEW, false)
        } else if v == V_LAKE {
            self.add_button(s, x, y, h, b"VIEW: LAKE", B_VIEW, false)
        } else if v == V_DEMO {
            self.add_button(s, x, y, h, b"VIEW: DEMO SCENE", B_VIEW, false)
        } else {
            self.add_button(s, x, y, h, b"VIEW: CUBE", B_VIEW, false)
        }
    }

    // ------------------------------------------------------------------ pattern panel

    /// The panel: song title and status, the clickable order list and the pattern rows
    /// around the current one (as many channels and columns as fit).
    pub(crate) fn draw_panel(&mut self, s: &Surface, cur: i32, pl: &Player) {
        let fs = self.fs;
        s.fill(0, 0, s.w, s.h, PANEL);
        s.vline(0, 0, s.h - 1, LINE, 256);
        s.hline(0, s.w - 1, 0, LINE, 256);
        let lh = 9 * fs;
        let chars = (s.w - 4 * fs) / (6 * fs);
        let st = if cur >= 0 {
            self.msgs[cur as usize].copy()
        } else {
            idle_msg(pl)
        };
        self.draw_title(s, pl);
        draw_status(s, &st, fs, chars >= 40);
        let oy = 3 * fs + 2 * lh + 2 * fs;
        let order = if cur >= 0 { st.order as i32 } else { -1 };
        let rows_used = self.draw_order_list(s, pl, oy, order);
        let py0 = oy + rows_used * lh + 3 * fs;
        s.hline(0, s.w - 1, py0 - 2 * fs, LINE, 256);
        draw_pattern_rows(s, pl, &st, cur >= 0, py0, fs, chars);
    }

    /// Song title, channels, length and "ENDE" once the song is over.
    fn draw_title(&self, s: &Surface, pl: &Player) {
        let fs = self.fs;
        let mut t = TextBuf::new();
        let mut i = 0;
        while i < 20 {
            let b = pl.mod_byte(i);
            if b == 0 {
                break;
            }
            t.ch(if b >= 32 && b < 127 { b as u8 } else { 32 });
            i += 1;
        }
        t.s(b"  ");
        t.num(pl.channels as u32, 1, 48);
        t.s(b" CH.  ");
        t.num(pl.song_len as u32, 1, 48);
        t.s(b" POS.");
        if self.ended {
            t.s(b"  END");
        }
        s.text_buf(3 * fs, 3 * fs, &t, fs, AMBER);
    }

    /// The order list as a grid of pattern numbers from y = `oy`, the entry `order`
    /// highlighted. Remembers the grid for [`Vis::pointer`] and returns its row count.
    fn draw_order_list(&mut self, s: &Surface, pl: &Player, oy: i32, order: i32) -> i32 {
        let fs = self.fs;
        let n = pl.song_len as i32;
        let cw = 6 * fs;
        let bw = 4 * cw;
        let bh = 9 * fs;
        let cols = imax((s.w - 4 * fs) / bw, 1);
        let mut k = 0;
        while k < n {
            let x = 3 * fs + (k % cols) * bw;
            let y = oy + (k / cols) * bh;
            let on = k == order;
            s.fill(
                x,
                y,
                bw - 2 * fs,
                bh - 2 * fs,
                if on { PHOSPHOR } else { rgb(18, 32, 26) },
            );
            let mut tt = TextBuf::new();
            tt.hex(pl.order_at(k as usize), 2);
            s.text_buf(
                x + cw / 2,
                y + fs,
                &tt,
                fs,
                if on { BLACK } else { PHOSPHOR },
            );
            k += 1;
        }
        // remember for hit tests (absolute coordinates)
        let off = (s.p as usize - (&raw const self.fb) as usize) as i32 / 4;
        self.ord_x = off % self.w + 3 * fs;
        self.ord_y = off / self.w + oy;
        self.ord_bw = bw;
        self.ord_bh = bh;
        self.ord_cols = cols;
        self.ord_n = n;
        (n + cols - 1) / cols
    }
}

/// Status shown while nothing is playing: start of the song, default speed and tempo.
fn idle_msg(pl: &Player) -> Msg {
    let mut m = MSG0;
    m.pattern = pl.order_at(0);
    m.speed = 6;
    m.tempo = 125;
    m
}

/// "POS 00  PAT 00  ROW 00  BPM 125/6" (abbreviated unless `roomy`).
fn draw_status(s: &Surface, m: &Msg, fs: i32, roomy: bool) {
    let mut t = TextBuf::new();
    t.s(b"POS ");
    t.num(m.order, 2, 48);
    if roomy {
        t.s(b"  PAT ");
    } else {
        t.s(b" PAT ");
    }
    t.hex(m.pattern, 2);
    if roomy {
        t.s(b"  ROW ");
    } else {
        t.s(b" ROW ");
    }
    t.num(m.row, 2, 48);
    t.s(if roomy { b"  BPM " } else { b" BPM  " });
    t.num(m.tempo, 1, 48);
    t.ch(47);
    t.num(m.speed, 1, 48);
    s.text_buf(3 * fs, 3 * fs + 9 * fs, &t, fs, TEXT);
}

/// The pattern rows around the current one from y = `py0`, centred on it, with the
/// widest cell format (note / + sample / + effect) that fits `chars` characters.
fn draw_pattern_rows(
    s: &Surface,
    pl: &Player,
    m: &Msg,
    playing: bool,
    py0: i32,
    fs: i32,
    chars: i32,
) {
    let lh = 9 * fs;
    let avail = (s.h - py0 - 2 * fs) / lh;
    if avail < 3 {
        return;
    }
    let nch = pl.channels as i32;
    let fmt = if chars >= 3 + nch * 11 {
        2
    } else if chars >= 3 + nch * 7 {
        1
    } else {
        0
    };
    let cell_chars = if fmt == 2 {
        11
    } else if fmt == 1 {
        7
    } else {
        4
    };
    let show = imin((chars - 3) / cell_chars, nch);
    let row = m.row as i32;
    let half = avail / 2;
    let mut line = 0;
    while line < avail {
        let r = row - half + line;
        let y = py0 + line * lh;
        if r >= 0 && r < 64 {
            let current = r == row && playing;
            if current {
                s.fill(0, y - fs, s.w, lh, PHOSPHOR);
            }
            let col = if current {
                BLACK
            } else if r % 4 == 0 {
                TEXT
            } else {
                MUTED
            };
            let mut tt = TextBuf::new();
            tt.num(r as u32, 2, 48);
            let mut c = 0;
            while c < show {
                tt.ch(32);
                cell_text(
                    &mut tt,
                    &pl.cell(m.pattern as usize, r as usize, c as usize),
                    fmt,
                );
                c += 1;
            }
            s.text_buf(3 * fs, y, &tt, fs, col);
        }
        line += 1;
    }
}

/// Appends a pattern cell: note; with `fmt` >= 1 the sample; with `fmt` >= 2 the effect.
pub(crate) fn cell_text(t: &mut TextBuf, c: &crate::replayer::Cell, fmt: i32) {
    t.note(c.period);
    if fmt >= 1 {
        t.ch(32);
        if c.sample != 0 {
            t.hex(c.sample, 2);
        } else {
            t.s(b"..");
        }
    }
    if fmt >= 2 {
        t.ch(32);
        if c.effect != 0 || c.param != 0 {
            t.hex(c.effect, 1);
            t.hex(c.param, 2);
        } else {
            t.s(b"...");
        }
    }
}
