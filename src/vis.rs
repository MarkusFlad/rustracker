//! Framebuffer renderer: the whole user interface of the "framebuffer edition", drawn in Rust.
//!
//! JavaScript only copies the worklet's scope/spectrum data into a slot (`vis_slot_ptr` +
//! `vis_commit`), calls `vis_frame` every animation frame and puts the returned RGBA buffer
//! on a canvas, and forwards pointer/keyboard input (`vis_pointer` / `vis_key`). Everything
//! visible - buttons, text, oscilloscope, spectrum, Mandelbrot, cube, pattern view - is
//! drawn here.
//!
//! This module holds the state ([`Vis`]), the data intake and the frame layout; the views
//! live in `vis_osc`, `vis_mandel`, `vis_spec` and `vis_cube`, the bar, panel and input in
//! `vis_ui`. Written in the same core-language-only subset as replayer.rs (no core
//! methods, no iterators, no slices, no allocation), so it also builds with
//! preview/minicore.rs.

use crate::gfx::*;
use crate::math::*;
use crate::replayer::Player;
use crate::spectrum::{unit_db, LogScale, Spectrum};
use crate::vis_spec::wf_color;

/// Framebuffer capacity in pixels (e.g. 1280 x 860 or 720 x 1520).
#[cfg(not(dos))]
pub const FB_PIXELS: usize = 1_100_000;
/// Framebuffer capacity in pixels: VGA 320 x 200 in the DOS build.
#[cfg(dos)]
pub const FB_PIXELS: usize = 64_000;
/// Scope values per channel in a slot.
pub const SCOPE_LEN: usize = 1024;
/// Channels with their own scope in a slot.
pub const VIS_CH: usize = 8;
/// Power values (FFT bins 0..=2048) in a slot.
pub const SPEC_LEN: usize = 2049;
pub(crate) const SLOTS: usize = 8;
/// Size of a slot in floats: channel scopes, master L and R, spectrum.
pub const SLOT_FLOATS: usize = (VIS_CH + 2) * SCOPE_LEN + SPEC_LEN;
/// Offset of the spectrum in a slot.
pub const SPEC_OFFSET: usize = (VIS_CH + 2) * SCOPE_LEN;
pub(crate) const NB: usize = 96;
pub(crate) const HIST_N: usize = 220;
pub(crate) const HIST_AGE: f64 = 4000.0;
pub(crate) const SG_W: usize = 512;
pub(crate) const SG_H: usize = 200;
pub(crate) const MB_MAX: usize = 170_000;
#[cfg(not(dos))]
pub(crate) const FACE_N: usize = 256;
/// DOS: cube faces of 128 x 128 pixels (a quarter of the work per frame).
#[cfg(dos)]
pub(crate) const FACE_N: usize = 128;
pub(crate) const PAL_LEN: usize = 2048;
pub(crate) const MAXW: usize = 1800;
pub(crate) const F_MIN: f64 = 30.0;
pub(crate) const DB_MIN: f64 = -96.0;

/// Action: nothing for JS to do.
pub const ACT_NONE: i32 = 0;
/// Action: toggle play/pause.
pub const ACT_PLAY: i32 = 1;
/// Action: stop and rewind.
pub const ACT_STOP: i32 = 2;
/// Action: open the file dialog.
pub const ACT_OPEN: i32 = 3;
/// Action: enter (arg 1) or leave (arg 0) fullscreen.
pub const ACT_FULLSCREEN: i32 = 4;
/// Action: jump to song position arg.
pub const ACT_SEEK: i32 = 5;
/// Action: song loop on (arg 1) or off (arg 0).
pub const ACT_LOOP: i32 = 7;

pub(crate) const B_PLAY: u32 = 1;
pub(crate) const B_STOP: u32 = 2;
pub(crate) const B_OPEN: u32 = 3;
pub(crate) const B_VIEW: u32 = 4;
pub(crate) const B_3D: u32 = 5;
pub(crate) const B_PAL: u32 = 6;
pub(crate) const B_LOOP: u32 = 7;
pub(crate) const B_FULL: u32 = 8;
pub(crate) const B_RES: u32 = 9;

pub(crate) const V_OSC: u32 = 0;
pub(crate) const V_MANDEL: u32 = 1;
pub(crate) const V_SPEC: u32 = 2;
pub(crate) const V_CUBE: u32 = 3;
pub(crate) const V_VOXEL: u32 = 4;
pub(crate) const V_LAKE: u32 = 5;
pub(crate) const V_DEMO: u32 = 6;
/// Number of views.
pub(crate) const N_VIEWS: u32 = 7;

/// What the worklet reported with one slot: when it is heard and where the replayer was.
pub(crate) struct Msg {
    pub(crate) valid: bool,
    pub(crate) t: f64,
    pub(crate) order: u32,
    pub(crate) row: u32,
    pub(crate) pattern: u32,
    pub(crate) speed: u32,
    pub(crate) tempo: u32,
    pub(crate) nch: u32,
    pub(crate) sr: f64,
}

impl Msg {
    /// A copy (the core-language subset has no `Clone`).
    pub(crate) fn copy(&self) -> Msg {
        Msg {
            valid: self.valid,
            t: self.t,
            order: self.order,
            row: self.row,
            pattern: self.pattern,
            speed: self.speed,
            tempo: self.tempo,
            nch: self.nch,
            sr: self.sr,
        }
    }
}

/// An empty message.
pub(crate) const MSG0: Msg = Msg {
    valid: false,
    t: 0.0,
    order: 0,
    row: 0,
    pattern: 0,
    speed: 0,
    tempo: 0,
    nch: 0,
    sr: 0.0,
};

// Places in the Mandelbrot set: cx, cy, smallest useful view height
pub(crate) const PLACE_CX: [f64; 5] = [
    -0.743643887037151,
    -0.77568377,
    0.2549870375144766,
    -0.10109636384562,
    -1.768778833,
];
pub(crate) const PLACE_CY: [f64; 5] = [
    0.131825904205330,
    0.13646737,
    -0.0005679790528465,
    0.95628651080914,
    -0.001738996,
];
pub(crate) const PLACE_MIN: [f64; 5] = [1e-4, 2e-6, 3e-6, 2e-5, 4e-7];

// Palettes: 6 stops each, position (0..1000) and colour
pub(crate) const PAL_POS: [u32; 18] = [
    0, 160, 420, 642, 857, 1000, 0, 200, 450, 620, 800, 1000, 0, 300, 550, 750, 900, 1000,
];
pub(crate) const PAL_COL: [u32; 18] = [
    rgb(0, 7, 100),
    rgb(32, 107, 203),
    rgb(237, 255, 255),
    rgb(255, 170, 0),
    rgb(0, 2, 0),
    rgb(0, 7, 100),
    rgb(11, 4, 32),
    rgb(90, 26, 140),
    rgb(255, 79, 123),
    rgb(255, 211, 107),
    rgb(47, 211, 198),
    rgb(11, 4, 32),
    rgb(0, 0, 0),
    rgb(58, 58, 58),
    rgb(232, 232, 232),
    rgb(138, 138, 138),
    rgb(30, 30, 30),
    rgb(0, 0, 0),
];

/// All renderer state. It is large (framebuffer, Mandelbrot buffers, face textures),
/// so it lives in a zero-initialised static (see [`Vis::new`]).
pub struct Vis {
    pub(crate) fb: [u32; FB_PIXELS],
    pub(crate) w: i32,
    pub(crate) h: i32,
    pub(crate) fs: i32,
    pub(crate) slots: [f32; SLOTS * SLOT_FLOATS],
    pub(crate) msgs: [Msg; SLOTS],
    pub(crate) wpos: usize,
    pub(crate) bands: [f32; SLOTS * NB],
    pub(crate) hist: [f32; HIST_N * NB],
    pub(crate) hist_t: [f64; HIST_N],
    pub(crate) hist_pos: usize,
    pub(crate) sg: [u32; SG_W * SG_H],
    pub(crate) sg_pos: usize,
    pub(crate) disp: [f32; NB],
    pub(crate) peak: [f32; NB],
    pub(crate) horizon: [i32; MAXW],
    pub(crate) newh: [i32; MAXW],
    // Mandelbrot buffers (palette indices, 0xffffffff = inside) and their views
    pub(crate) mb: [u32; 2 * MB_MAX],
    pub(crate) mb_front: usize,
    pub(crate) f_valid: bool,
    pub(crate) f_re0: f64,
    pub(crate) f_im0: f64,
    pub(crate) f_step: f64,
    pub(crate) f_w: i32,
    pub(crate) f_h: i32,
    pub(crate) b_active: bool,
    pub(crate) b_re0: f64,
    pub(crate) b_im0: f64,
    pub(crate) b_step: f64,
    pub(crate) b_w: i32,
    pub(crate) b_h: i32,
    pub(crate) b_row: i32,
    pub(crate) b_frames: u32,
    pub(crate) b_maxit: u32,
    pub(crate) b_face: bool,
    pub(crate) workers: bool,
    pub(crate) job_gen: u32,
    pub(crate) job_taken: bool,
    pub(crate) job_rows: i32,
    pub(crate) mb_factor: f64,
    pub(crate) pal: [u32; PAL_LEN],
    pub(crate) pal_idx: u32,
    pub(crate) face: [u32; 6 * FACE_N * FACE_N],
    // music director (Mandelbrot)
    pub(crate) d_order: i32,
    pub(crate) d_place: usize,
    pub(crate) d_z: f64,
    pub(crate) d_prev_e: f64,
    pub(crate) d_shift: f64,
    pub(crate) d_shift_shown: f64,
    pub(crate) d_flash: f64,
    pub(crate) d_rowkey: u32,
    pub(crate) inten: f64,
    pub(crate) peak_rms: f64,
    pub(crate) v_cx: f64,
    pub(crate) v_cy: f64,
    pub(crate) v_size: f64,
    pub(crate) v_rot: f64,
    // ui state
    pub(crate) view: u32,
    /// Views the view button skips (bit `v` for view `v`). Zero by default: a non-zero
    /// initial value would move the whole renderer state from .bss to .data, which makes
    /// the DOS image several megabytes larger.
    pub(crate) view_off: u32,
    pub(crate) spec3d: bool,
    pub(crate) fullscreen: bool,
    pub(crate) playing: bool,
    pub(crate) looping: bool,
    pub(crate) ended: bool,
    pub(crate) last_t: f64,
    pub(crate) t0: f64,
    pub(crate) pulse: f64,
    pub(crate) btn_x: [i32; 12],
    pub(crate) btn_y: [i32; 12],
    pub(crate) btn_w: [i32; 12],
    pub(crate) btn_h: [i32; 12],
    pub(crate) btn_id: [u32; 12],
    pub(crate) nbtn: usize,
    pub(crate) bar_rows: i32,
    pub(crate) bar_y: i32,
    pub(crate) ord_x: i32,
    pub(crate) ord_y: i32,
    pub(crate) ord_bw: i32,
    pub(crate) ord_bh: i32,
    pub(crate) ord_cols: i32,
    pub(crate) ord_n: i32,
    pub(crate) action_arg: i32,
    pub(crate) ready: bool,
    pub(crate) d_has_e: bool,
    // voxel view: height map and lighting of the front Mandelbrot buffer
    pub(crate) mb_done: u32,
    pub(crate) vx_seen: u32,
    pub(crate) vx_ready: bool,
    pub(crate) vx_hist: [u32; crate::vis_voxel::HIST_BINS],
    pub(crate) vx_cdf: [u32; crate::vis_voxel::HIST_BINS],
    pub(crate) vx_h: [u16; MB_MAX],
    pub(crate) vx_l: [u8; MB_MAX],
    pub(crate) vx_c: [u32; MB_MAX],
    /// Voxel views: smooth surface instead of blocky voxels (the default).
    pub(crate) vx_fine: bool,
    // lake view: wave buffers, mask, height map and colours
    pub(crate) lk_a: [f32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_b: [f32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_flip: bool,
    pub(crate) lk_mask: [u8; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    /// 1 where a shore cell lies within the beach width of the water (sand colour).
    pub(crate) lk_beach: [u8; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_h: [u16; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_c: [u32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_ready: bool,
    pub(crate) lk_acc: f64,
    pub(crate) lk_bass_avg: f64,
    pub(crate) lk_bass: f64,
    pub(crate) lk_hit_t: f64,
    pub(crate) lk_slot_t: f64,
    pub(crate) lk_rowkey: u32,
    pub(crate) lk_drops: u32,
    // fine mode: smooth shoreline layers
    pub(crate) lk_edge: [f32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_lh: [u16; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_lc: [u32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_sc: [u32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_wh: [u16; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    pub(crate) lk_wc: [u32; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
    // demo-scene view: scroll text, starfield, note flashes, tables of the effects
    pub(crate) dm_ready: bool,
    pub(crate) dm_text: [u8; crate::vis_demo::DM_TEXT],
    pub(crate) dm_len: usize,
    pub(crate) dm_scroll: f64,
    pub(crate) dm_stars: [f32; 3 * crate::vis_demo::DM_STARS],
    pub(crate) dm_seed: u32,
    pub(crate) dm_flash: [f32; 8],
    pub(crate) dm_rowkey: u32,
    pub(crate) dm_pulse: f64,
    pub(crate) dm_sin: [i32; 256],
    pub(crate) dm_plasma_pal: [u32; 256],
    pub(crate) dm_tex_pal: [u32; 256],
    pub(crate) dm_tex: [u8; 65536],
}

impl Vis {
    /// All-zero initial state, so the (large) static is placed in .bss; the rest is set up in the
    /// first [`Vis::frame`].
    ///
    /// # Returns
    /// A renderer that has not drawn yet.
    pub const fn new() -> Vis {
        Vis {
            fb: [0; FB_PIXELS],
            w: 0,
            h: 0,
            fs: 0,
            slots: [0.0; SLOTS * SLOT_FLOATS],
            msgs: [MSG0; SLOTS],
            wpos: 0,
            bands: [0.0; SLOTS * NB],
            hist: [0.0; HIST_N * NB],
            hist_t: [0.0; HIST_N],
            hist_pos: 0,
            sg: [0; SG_W * SG_H],
            sg_pos: 0,
            disp: [0.0; NB],
            peak: [0.0; NB],
            horizon: [0; MAXW],
            newh: [0; MAXW],
            mb: [0; 2 * MB_MAX],
            mb_front: 0,
            f_valid: false,
            f_re0: 0.0,
            f_im0: 0.0,
            f_step: 0.0,
            f_w: 0,
            f_h: 0,
            b_active: false,
            b_re0: 0.0,
            b_im0: 0.0,
            b_step: 0.0,
            b_w: 0,
            b_h: 0,
            b_row: 0,
            b_frames: 0,
            b_maxit: 0,
            b_face: false,
            workers: false,
            job_gen: 0,
            job_taken: false,
            job_rows: 0,
            mb_factor: 0.0,
            pal: [0; PAL_LEN],
            pal_idx: 0,
            face: [0; 6 * FACE_N * FACE_N],
            d_order: 0,
            d_place: 0,
            d_z: 0.0,
            d_prev_e: 0.0,
            d_shift: 0.0,
            d_shift_shown: 0.0,
            d_flash: 0.0,
            d_rowkey: 0,
            inten: 0.0,
            peak_rms: 0.0,
            v_cx: 0.0,
            v_cy: 0.0,
            v_size: 0.0,
            v_rot: 0.0,
            view: 0,
            view_off: 0,
            spec3d: false,
            fullscreen: false,
            playing: false,
            looping: false,
            ended: false,
            last_t: 0.0,
            t0: 0.0,
            pulse: 0.0,
            btn_x: [0; 12],
            btn_y: [0; 12],
            btn_w: [0; 12],
            btn_h: [0; 12],
            btn_id: [0; 12],
            nbtn: 0,
            bar_rows: 0,
            bar_y: 0,
            ord_x: 0,
            ord_y: 0,
            ord_bw: 0,
            ord_bh: 0,
            ord_cols: 0,
            ord_n: 0,
            action_arg: 0,
            ready: false,
            d_has_e: false,
            mb_done: 0,
            vx_seen: 0,
            vx_ready: false,
            vx_hist: [0; crate::vis_voxel::HIST_BINS],
            vx_cdf: [0; crate::vis_voxel::HIST_BINS],
            vx_h: [0; MB_MAX],
            vx_l: [0; MB_MAX],
            vx_c: [0; MB_MAX],
            vx_fine: false,
            lk_a: [0.0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_b: [0.0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_flip: false,
            lk_mask: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_beach: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_h: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_c: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_ready: false,
            lk_acc: 0.0,
            lk_bass_avg: 0.0,
            lk_bass: 0.0,
            lk_hit_t: 0.0,
            lk_slot_t: 0.0,
            lk_rowkey: 0,
            lk_drops: 0,
            lk_edge: [0.0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_lh: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_lc: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_sc: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_wh: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            lk_wc: [0; crate::vis_lake::LAKE_N * crate::vis_lake::LAKE_N],
            dm_ready: false,
            dm_text: [0; crate::vis_demo::DM_TEXT],
            dm_len: 0,
            dm_scroll: 0.0,
            dm_stars: [0.0; 3 * crate::vis_demo::DM_STARS],
            dm_seed: 0,
            dm_flash: [0.0; 8],
            dm_rowkey: 0,
            dm_pulse: 0.0,
            dm_sin: [0; 256],
            dm_plasma_pal: [0; 256],
            dm_tex_pal: [0; 256],
            dm_tex: [0; 65536],
        }
    }

    // ------------------------------------------------------------------ data from the worklet

    /// Where the caller copies the next worklet message (see `vis_slot_ptr` in `api.rs` for the
    /// layout). Complete it with [`Vis::commit`].
    ///
    /// # Returns
    /// Pointer to [`SLOT_FLOATS`] writable floats.
    pub fn slot_ptr(&mut self) -> *mut f32 {
        ((&raw mut self.slots) as usize + self.wpos * SLOT_FLOATS * 4) as *mut f32
    }

    /// Scope value `i` of slot `k` (channels, then master L and R, 1024 values each).
    pub(crate) fn sv(&self, k: usize, i: usize) -> f32 {
        self.slots[k * SLOT_FLOATS + i]
    }

    /// The spectrum of slot `k` as a [`Spectrum`] view.
    pub(crate) fn spectrum(&self, k: usize) -> Spectrum {
        let p = (&raw const self.slots) as usize + (k * SLOT_FLOATS + SPEC_OFFSET) * 4;
        unsafe { Spectrum::new(p as *const f32, SPEC_LEN, self.msgs[k].sr / 4096.0) }
    }

    /// Completes the slot written through [`Vis::slot_ptr`]. Derives the level, the spectrum bands,
    /// a spectrogram column and a 3D history row.
    ///
    /// # Arguments
    /// * `t` - when its audio is heard, in ms (same clock as `frame`)
    /// * `order` - song position
    /// * `row` - row, 0..63
    /// * `pattern` - pattern number
    /// * `speed` - ticks per row (0 means 6)
    /// * `tempo` - BPM (0 means 125)
    /// * `nch` - channels in the slot (at most [`VIS_CH`])
    /// * `sr` - sample rate in Hz
    pub fn commit(
        &mut self,
        t: f64,
        order: u32,
        row: u32,
        pattern: u32,
        speed: u32,
        tempo: u32,
        nch: u32,
        sr: f64,
    ) {
        let k = self.wpos;
        let nch = if nch as usize > VIS_CH {
            VIS_CH as u32
        } else {
            nch
        };
        let speed = if speed == 0 { 6 } else { speed };
        let tempo = if tempo == 0 { 125 } else { tempo };
        self.update_level(k, nch);
        self.msgs[k] = Msg {
            valid: true,
            t,
            order,
            row,
            pattern,
            speed,
            tempo,
            nch,
            sr,
        };
        self.compute_bands(k);
        self.write_spectrogram_column(k);
        self.push_history(k, t);
        self.wpos = (self.wpos + 1) % SLOTS;
    }

    /// RMS of the master signal of slot `k`, the slowly decaying peak level and the
    /// smoothed intensity (0..1) used by the music director.
    fn update_level(&mut self, k: usize, nch: u32) {
        let ml = nch as usize * SCOPE_LEN;
        let mut e = 0.0;
        let mut i = 0;
        while i < SCOPE_LEN {
            let l = self.sv(k, ml + i) as f64;
            let r = self.sv(k, ml + SCOPE_LEN + i) as f64;
            e += l * l + r * r;
            i += 1;
        }
        let rms = sqrt(e / (2.0 * SCOPE_LEN as f64));
        self.peak_rms = maxf(maxf(rms, self.peak_rms * 0.9995), 0.02);
        self.inten += (minf(1.0, rms / self.peak_rms) - self.inten) * 0.04;
    }

    /// The highest frequency shown for slot `k` (20 kHz or Nyquist).
    pub(crate) fn fmax(&self, k: usize) -> f64 {
        minf(20000.0, self.msgs[k].sr / 2.0)
    }

    /// `NB` log-frequency bands of slot `k`, normalised dB (0 = -96 dB, 1 = 0 dB).
    fn compute_bands(&mut self, k: usize) {
        let sp = self.spectrum(k);
        let ratio = LogScale::new(F_MIN, self.fmax(k)).ratio(NB);
        let mut f0 = F_MIN;
        let mut b = 0;
        while b < NB {
            let f1 = f0 * ratio;
            self.bands[k * NB + b] = unit_db(sp.band(f0, f1), DB_MIN) as f32;
            f0 = f1;
            b += 1;
        }
    }

    /// One spectrogram column (top = high frequencies) from slot `k`.
    fn write_spectrogram_column(&mut self, k: usize) {
        let sp = self.spectrum(k);
        let fmax = self.fmax(k);
        let r2 = LogScale::new(F_MIN, fmax).ratio(SG_H);
        let mut fhi = fmax;
        let mut y = 0;
        while y < SG_H {
            let flo = fhi / r2;
            self.sg[y * SG_W + self.sg_pos] = wf_color(unit_db(sp.band(flo, fhi), DB_MIN));
            fhi = flo;
            y += 1;
        }
        self.sg_pos = (self.sg_pos + 1) % SG_W;
    }

    /// A 3D history row: the bands with fast attack and slow release.
    fn push_history(&mut self, k: usize, t: f64) {
        let prev = (self.hist_pos + HIST_N - 1) % HIST_N;
        let dtm = clampf((t - self.hist_t[prev]) / 1000.0, 0.0, 0.1);
        let mut b = 0;
        while b < NB {
            let nv = self.bands[k * NB + b];
            let ov = self.hist[prev * NB + b] - (dtm * 0.9) as f32;
            self.hist[self.hist_pos * NB + b] = if nv > ov { nv } else { ov };
            b += 1;
        }
        self.hist_t[self.hist_pos] = t;
        self.hist_pos = (self.hist_pos + 1) % HIST_N;
    }

    /// The slot whose audio is being heard at `now` (newest with t <= now; the oldest
    /// one if all are in the future), or -1 if there is none.
    pub(crate) fn cur_msg(&self, now: f64) -> i32 {
        let mut best = -1;
        let mut best_t = -1.0e18;
        let mut oldest = -1;
        let mut oldest_t = 1.0e18;
        let mut k = 0;
        while k < SLOTS {
            let m = &self.msgs[k];
            if m.valid && m.t <= now && m.t > best_t {
                best = k as i32;
                best_t = m.t;
            }
            if m.valid && m.t < oldest_t {
                oldest = k as i32;
                oldest_t = m.t;
            }
            k += 1;
        }
        if best >= 0 {
            best
        } else {
            oldest
        }
    }

    // ------------------------------------------------------------------ state from JS

    /// Forgets all audio data and restarts the Mandelbrot journeys (new song or stop).
    pub fn reset_song(&mut self) {
        let mut k = 0;
        while k < SLOTS {
            self.msgs[k].valid = false;
            k += 1;
        }
        self.d_order = -1;
        self.ended = false;
        self.dm_ready = false;
    }

    /// Tells the renderer whether the song is playing (button label, animation of time-based
    /// views).
    ///
    /// # Arguments
    /// * `p` - `true` while playing
    pub fn set_playing(&mut self, p: bool) {
        self.playing = p;
        if p {
            self.ended = false;
        }
    }

    /// Song events from JS.
    ///
    /// # Arguments
    /// * `code` - 1 = the song has ended, 2 = a new song was loaded; others are ignored
    pub fn event(&mut self, code: i32) {
        if code == 1 {
            self.ended = true;
            self.playing = false;
        } else if code == 2 {
            self.reset_song();
        }
    }

    /// Width of the last frame.
    ///
    /// # Returns
    /// Pixels, after clamping to the framebuffer.
    pub fn width(&self) -> i32 {
        self.w
    }

    /// Height of the last frame.
    ///
    /// # Returns
    /// Pixels, after clamping to the framebuffer.
    pub fn height(&self) -> i32 {
        self.h
    }

    /// The argument of the last action returned by [`Vis::pointer`] or [`Vis::key`].
    ///
    /// # Returns
    /// Song position for `ACT_SEEK`, 1/0 for `ACT_LOOP` and `ACT_FULLSCREEN`.
    pub fn action_arg(&self) -> i32 {
        self.action_arg
    }

    // ------------------------------------------------------------------ frame

    /// Draws a complete frame.
    ///
    /// # Arguments
    /// * `now` - time in ms (performance.now())
    /// * `w` - wanted width in pixels
    /// * `h` - wanted height in pixels; the size is clamped to the framebuffer, see [`Vis::width`]/[`Vis::height`]
    /// * `css` - framebuffer pixels per CSS pixel, so text and buttons get a sensible physical size
    /// * `pl` - the player with the song (patterns, title, order list)
    ///
    /// # Returns
    /// Pointer to the RGBA pixels (`width()`·`height()` u32, rows without padding).
    pub fn frame(&mut self, now: f64, w: i32, h: i32, css: f64, pl: &Player) -> *const u32 {
        let (s, dt, cur) = self.begin_frame(now, w, h, css);
        let bar_h = self.draw_bar(&s, 13 * self.fs); // the bar may wrap onto more rows
        let (view, panel) = self.layout(bar_h);
        self.draw_view(&s.sub(view.x, view.y, view.w, view.h), now, dt, cur, pl);
        if panel.w > 0 && panel.h > 0 {
            self.draw_panel(&s.sub(panel.x, panel.y, panel.w, panel.h), cur, pl);
        }
        (&raw const self.fb) as *const u32
    }

    /// Draws a single view filling the whole frame, without bar and panel: for pages that
    /// build their own user interface around it (the classic edition uses it for the voxel
    /// views).
    ///
    /// # Arguments
    /// * `view` - the view: 0 oscilloscope, 1 Mandelbrot, 2 spectrum, 3 cube, 4 voxel
    ///   landscape, 5 lake, 6 demo scene (larger values: lake)
    /// * `now` - time in ms (performance.now())
    /// * `w` - wanted width in pixels
    /// * `h` - wanted height in pixels (clamped like in [`Vis::frame`])
    /// * `css` - framebuffer pixels per CSS pixel (size of the text in the view)
    /// * `pl` - the player with the song
    ///
    /// # Returns
    /// Pointer to the RGBA pixels (`width()`·`height()` u32).
    pub fn view_frame(
        &mut self,
        view: u32,
        now: f64,
        w: i32,
        h: i32,
        css: f64,
        pl: &Player,
    ) -> *const u32 {
        self.view = if view < N_VIEWS { view } else { V_LAKE };
        let (s, dt, cur) = self.begin_frame(now, w, h, css);
        self.draw_view(&s, now, dt, cur, pl);
        (&raw const self.fb) as *const u32
    }

    /// Common start of a frame: size, first-time setup, time step, font scale, cleared
    /// surface and the slot being heard.
    fn begin_frame(&mut self, now: f64, w: i32, h: i32, css: f64) -> (Surface, f64, i32) {
        self.set_size(w, h);
        self.init_once(now);
        let dt = clampf((now - self.last_t) / 1000.0, 0.0, 0.1);
        self.last_t = now;
        self.fs = font_scale(css, self.w, self.h);
        let s = Surface::new((&raw mut self.fb) as *mut u32, self.w, self.h);
        s.clear(BG);
        let cur = self.cur_msg(now);
        (s, dt, cur)
    }

    /// Smooth surface (`true`) or blocky voxels in the voxel views.
    ///
    /// # Arguments
    /// * `on` - `true` for the smooth surface
    pub fn set_fine(&mut self, on: bool) {
        self.vx_fine = on;
    }

    /// Chooses the views the view button steps through; the oscilloscope always stays.
    ///
    /// # Arguments
    /// * `mask` - bit `v` set: view `v` is included (see [`vis_view`](crate::api::vis_view))
    pub fn set_views(&mut self, mask: u32) {
        self.view_off = ((1 << N_VIEWS) - 1) & (0x7FFF_FFFF ^ (mask | 1));
        if self.view_off & (1 << self.view) != 0 {
            self.view = V_OSC;
            self.f_valid = false;
        }
    }

    /// The view after `v` among the enabled ones.
    pub(crate) fn next_view(&self, v: u32) -> u32 {
        let mut n = (v + 1) % N_VIEWS;
        while self.view_off & (1 << n) != 0 {
            n = (n + 1) % N_VIEWS;
        }
        n
    }

    /// Clamps the frame size to the framebuffer.
    fn set_size(&mut self, w: i32, h: i32) {
        let mut w = imax(w, 64);
        let mut h = imax(h, 64);
        if (w as usize) * (h as usize) > FB_PIXELS {
            h = (FB_PIXELS / w as usize) as i32;
        }
        if w as usize > MAXW {
            w = MAXW as i32;
        }
        self.w = w;
        self.h = h;
    }

    /// Initial values that are not zero (the static starts all-zero, see `new`).
    fn init_once(&mut self, now: f64) {
        if self.ready {
            return;
        }
        self.ready = true;
        self.pal_idx = 0;
        self.make_palette();
        self.mb_factor = 2.0;
        self.d_place = 4;
        self.inten = 0.5;
        self.peak_rms = 0.05;
        self.v_cx = PLACE_CX[0];
        self.v_cy = PLACE_CY[0];
        self.v_size = 3.0;
        self.d_order = -1;
        self.d_rowkey = 0xffff_ffff;
        self.vx_fine = true;
        self.t0 = now;
    }

    /// Splits the area below the bar into the view and the panel: side by side on wide
    /// screens, stacked on tall ones, view only in fullscreen mode.
    fn layout(&self, bar_h: i32) -> (Rect, Rect) {
        let (w, h, fs) = (self.w, self.h, self.fs);
        let none = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        if self.fullscreen {
            return (
                Rect {
                    x: 0,
                    y: bar_h,
                    w,
                    h: h - bar_h,
                },
                none,
            );
        }
        if w as f64 >= h as f64 * 1.25 {
            let panel = imin(imax(w * 34 / 100, 60 * fs), w / 2);
            let vw = w - panel;
            return (
                Rect {
                    x: 0,
                    y: bar_h,
                    w: vw,
                    h: h - bar_h,
                },
                Rect {
                    x: vw,
                    y: bar_h,
                    w: panel,
                    h: h - bar_h,
                },
            );
        }
        let vh = (h - bar_h) * 56 / 100;
        (
            Rect {
                x: 0,
                y: bar_h,
                w,
                h: vh,
            },
            Rect {
                x: 0,
                y: bar_h + vh,
                w,
                h: h - bar_h - vh,
            },
        )
    }

    /// Draws the selected view into `v`.
    fn draw_view(&mut self, v: &Surface, now: f64, dt: f64, cur: i32, pl: &Player) {
        if self.view == V_OSC {
            self.draw_osc(v, cur, pl);
        } else if self.view == V_MANDEL {
            self.direct(now, dt, cur, pl);
            self.draw_mandel(v, true);
        } else if self.view == V_SPEC && self.spec3d {
            self.draw_spec3(v, now, cur, true);
        } else if self.view == V_SPEC {
            self.draw_spec2(v, dt, cur, true);
        } else if self.view == V_CUBE {
            self.direct(now, dt, cur, pl);
            self.draw_cube(v, now, dt, cur, pl);
        } else if self.view == V_VOXEL {
            self.direct(now, dt, cur, pl);
            self.draw_voxel(v, now, dt);
        } else if self.view == V_LAKE {
            self.draw_lake(v, now, dt, cur);
        } else {
            self.draw_demo(v, now, dt, cur, pl);
        }
    }
}

/// A rectangle in framebuffer pixels.
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

/// Font scale (pixels per font dot) for `css` framebuffer pixels per CSS pixel, keeping
/// at least ~34 characters across the narrow side.
fn font_scale(css: f64, w: i32, h: i32) -> i32 {
    let mut fs = (clampf(css, 0.5, 4.0) * 1.6 + 0.5) as i32;
    while fs > 1 && imin(w, h) / (6 * fs) < 34 {
        fs -= 1;
    }
    imax(fs, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_button_skips_disabled_views() {
        static mut V: Vis = Vis::new();
        let v = unsafe { &mut *(&raw mut V) };
        assert_eq!(v.next_view(V_OSC), V_MANDEL);
        v.set_views(0x6D); // without Mandelbrot (1) and voxel (4)
        let mut seen = [false; N_VIEWS as usize];
        let mut cur = V_OSC;
        for _ in 0..N_VIEWS {
            cur = v.next_view(cur);
            seen[cur as usize] = true;
        }
        assert!(!seen[V_MANDEL as usize] && !seen[V_VOXEL as usize]);
        assert!(
            seen[V_SPEC as usize]
                && seen[V_CUBE as usize]
                && seen[V_LAKE as usize]
                && seen[V_DEMO as usize]
        );
        v.view = V_VOXEL;
        v.set_views(0); // the oscilloscope always stays; a disabled current view falls back
        assert_eq!(v.view, V_OSC);
        assert_eq!(v.next_view(V_OSC), V_OSC);
    }
}
