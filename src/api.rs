//! C ABI for JavaScript – no wasm-bindgen needed. Like replayer.rs, this file
//! only uses the core language so it also builds with preview/minicore.rs.
//!
//! Audio (inside the AudioWorklet):
//! ```text
//! buf = mp_mod_buffer(); copy MOD bytes there (max mp_mod_capacity());
//! mp_load(len, sampleRate)
//! per block: ptr = mp_render(frames)  -> interleaved stereo f32 (2*frames)
//!            mp_channels_ptr()        -> planar per-channel f32 (channels*frames)
//!            mp_spectrum(4096)        -> power spectrum of the last 4096 samples
//! ```
//! The `vis_*` functions drive the framebuffer renderer ([`crate::vis`]) of www-fb.
//! All state lives in statics: one player, one renderer per WebAssembly instance.

use crate::fft::{Fft, FFT_MAX};
use crate::replayer::{Player, MAX_CH};
use crate::vis::Vis;

/// Largest MOD file that can be loaded (bytes).
#[cfg(not(dos))]
pub const MOD_CAPACITY: usize = 4 * 1024 * 1024;
/// The DOS build loads songs in place ([`mp_load_ptr`]) and needs no copy buffer.
#[cfg(dos)]
pub const MOD_CAPACITY: usize = 4;
/// Most frames one `mp_render` call produces.
pub const MAX_FRAMES: usize = 4096;

static mut PLAYER: Player = Player::empty();
static mut MOD_BUF: [u8; MOD_CAPACITY] = [0; MOD_CAPACITY];
static mut MIX: [f32; 2 * MAX_FRAMES] = [0.0; 2 * MAX_FRAMES];
static mut CHAN: [f32; MAX_CH * MAX_FRAMES] = [0.0; MAX_CH * MAX_FRAMES];
// the last FFT_MAX mono samples of the mix, for the spectrum
static mut RING: [f32; FFT_MAX] = [0.0; FFT_MAX];
static mut RING_POS: usize = 0;
static mut FFT: Fft = Fft::empty();
static mut VIS: Vis = Vis::new();

/// The one player instance.
fn player() -> &'static mut Player {
    unsafe { &mut *(&raw mut PLAYER) }
}

/// Where JS copies the MOD file before [`mp_load`].
///
/// # Returns
/// Pointer to [`MOD_CAPACITY`] writable bytes.
#[no_mangle]
pub extern "C" fn mp_mod_buffer() -> *mut u8 {
    (&raw mut MOD_BUF) as *mut u8
}

/// Size of the buffer of [`mp_mod_buffer`].
///
/// # Returns
/// Bytes ([`MOD_CAPACITY`]).
#[no_mangle]
pub extern "C" fn mp_mod_capacity() -> usize {
    MOD_CAPACITY
}

/// Parses the MOD previously copied into [`mp_mod_buffer`] and starts at position 0.
///
/// # Arguments
/// * `len` - length of the file in bytes
/// * `sample_rate` - output sample rate in Hz
///
/// # Returns
/// The channel count (> 0) or an error code: -3 larger than the buffer, -1 too short, -2
/// unsupported channel count.
#[no_mangle]
pub extern "C" fn mp_load(len: usize, sample_rate: f32) -> i32 {
    if len > MOD_CAPACITY {
        return -3;
    }
    player().load((&raw const MOD_BUF) as *const u8, len, sample_rate)
}

/// Parses a MOD that stays where it is (the DOS build loads songs straight into memory) and
/// starts at position 0.
///
/// # Arguments
/// * `m` - start of the MOD file; it must stay valid and unchanged while it plays
/// * `len` - length of the file in bytes
/// * `sample_rate` - output sample rate in Hz
///
/// # Returns
/// The channel count (> 0) or an error code like [`mp_load`].
///
/// # Safety
/// `m` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn mp_load_ptr(m: *const u8, len: usize, sample_rate: f32) -> i32 {
    player().load(m, len, sample_rate)
}

/// Renders the next frames and appends their mono mix to the spectrum ring.
///
/// # Arguments
/// * `frames` - number of frames, at most [`MAX_FRAMES`] (more are cut)
///
/// # Returns
/// Pointer to 2·frames interleaved stereo f32, valid until the next call.
#[no_mangle]
pub extern "C" fn mp_render(frames: usize) -> *const f32 {
    let n = if frames > MAX_FRAMES {
        MAX_FRAMES
    } else {
        frames
    };
    let mix = (&raw mut MIX) as *mut f32;
    player().render(mix, (&raw mut CHAN) as *mut f32, n);
    unsafe { push_mono(mix, n) };
    mix as *const f32
}

/// Appends the mono mix of `n` interleaved stereo frames to the spectrum ring.
///
/// # Safety
/// `mix` must hold `2 * n` floats.
unsafe fn push_mono(mix: *const f32, n: usize) {
    let ring = &mut *(&raw mut RING);
    let mut pos = RING_POS;
    let mut i = 0;
    while i < n {
        let l = *((mix as usize + 2 * i * 4) as *const f32);
        let r = *((mix as usize + (2 * i + 1) * 4) as *const f32);
        ring[pos] = (l + r) * 0.5;
        pos = (pos + 1) % FFT_MAX;
        i += 1;
    }
    RING_POS = pos;
}

/// `v` limited to `lo ..= hi`.
fn clamp32(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Per-channel output of the last [`mp_render`].
///
/// # Returns
/// Pointer to channels·frames f32, one block of `frames` values per channel.
#[no_mangle]
pub extern "C" fn mp_channels_ptr() -> *const f32 {
    (&raw const CHAN) as *const f32
}

/// Channel count of the loaded song.
///
/// # Returns
/// 4, 6, 8, ... (0 without a song).
#[no_mangle]
pub extern "C" fn mp_num_channels() -> u32 {
    player().channels as u32
}

/// Current song position.
///
/// # Returns
/// Index into the order list.
#[no_mangle]
pub extern "C" fn mp_order() -> u32 {
    player().order_pos as u32
}

/// Current row.
///
/// # Returns
/// 0..63.
#[no_mangle]
pub extern "C" fn mp_row() -> u32 {
    player().row as u32
}

/// Pattern at the current song position.
///
/// # Returns
/// The pattern number.
#[no_mangle]
pub extern "C" fn mp_pattern() -> u32 {
    player().pattern() as u32
}

/// Length of the order list.
///
/// # Returns
/// Number of song positions.
#[no_mangle]
pub extern "C" fn mp_song_length() -> u32 {
    player().song_len as u32
}

/// Current speed.
///
/// # Returns
/// Ticks per row.
#[no_mangle]
pub extern "C" fn mp_speed() -> u32 {
    player().speed
}

/// Current tempo.
///
/// # Returns
/// BPM (125 = 50 ticks per second).
#[no_mangle]
pub extern "C" fn mp_tempo() -> u32 {
    player().tempo
}

/// How often the song has reached its end (wrapped around).
///
/// # Returns
/// The count since [`mp_load`].
#[no_mangle]
pub extern "C" fn mp_loops() -> u32 {
    player().loops
}

/// Jumps to a song position (row 0).
///
/// # Arguments
/// * `order` - song position; out of range means 0
#[no_mangle]
pub extern "C" fn mp_set_position(order: u32) {
    player().set_position(order as usize)
}

/// Sets the stereo separation.
///
/// # Arguments
/// * `sep` - 0 = mono ..= 1 = hard Amiga panning (clamped)
#[no_mangle]
pub extern "C" fn mp_set_separation(sep: f32) {
    player().separation = clamp32(sep, 0.0, 1.0);
}

/// Sets the output gain.
///
/// # Arguments
/// * `gain` - 0 ..= 2 (clamped), default 0.5
#[no_mangle]
pub extern "C" fn mp_set_gain(gain: f32) {
    player().gain = clamp32(gain, 0.0, 2.0);
}

/// FFT (Hann window) of the newest mono samples of the mix.
///
/// # Arguments
/// * `size` - FFT size: a power of two from 16 to 4096
///
/// # Returns
/// Pointer to size/2+1 power values (amplitude², 1.0 = full-scale sine), valid until the next call,
/// or null for an unsupported size.
#[no_mangle]
pub extern "C" fn mp_spectrum(size: usize) -> *const f32 {
    let f = unsafe { &mut *(&raw mut FFT) };
    if !f.setup(size) {
        return 0 as *const f32;
    }
    unsafe { f.analyse_ring((&raw const RING) as *const f32, FFT_MAX, RING_POS) };
    (&raw const f.power) as *const f32
}

// ------------------------------------------------------------------ framebuffer renderer
// Used by the main-thread instance of the framebuffer edition (www-fb). The worklet's scope
// and spectrum data are copied into a slot, then each animation frame draws the whole UI.

/// The one renderer instance.
fn vis() -> &'static mut Vis {
    unsafe { &mut *(&raw mut VIS) }
}

/// Where JS copies the next worklet message: channel scopes (1024 floats each, at most 8 channels),
/// then master L and R, then at [`vis_spec_offset`] the 2049 spectrum values. Complete it with
/// [`vis_commit`].
///
/// # Returns
/// Pointer to the slot's floats.
#[no_mangle]
pub extern "C" fn vis_slot_ptr() -> *mut f32 {
    vis().slot_ptr()
}

/// Offset of the spectrum within a slot.
///
/// # Returns
/// Floats from the start of the slot.
#[no_mangle]
pub extern "C" fn vis_spec_offset() -> u32 {
    crate::vis::SPEC_OFFSET as u32
}

/// The slot is filled (see [`Vis::commit`]).
///
/// # Arguments
/// * `t` - when it is heard: performance.now() + output latency, in ms
/// * `order` - song position
/// * `row` - row, 0..63
/// * `pattern` - pattern number
/// * `speed` - ticks per row
/// * `tempo` - BPM
/// * `nch` - channels copied into the slot
/// * `sample_rate` - sample rate in Hz
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn vis_commit(
    t: f64,
    order: u32,
    row: u32,
    pattern: u32,
    speed: u32,
    tempo: u32,
    nch: u32,
    sample_rate: f64,
) {
    vis().commit(t, order, row, pattern, speed, tempo, nch, sample_rate)
}

/// Draws a frame.
///
/// # Arguments
/// * `now` - time in ms (performance.now())
/// * `w` - wanted width in pixels
/// * `h` - wanted height; clamped to the buffer, see [`vis_width`]/[`vis_height`]
/// * `css` - framebuffer pixels per CSS pixel
///
/// # Returns
/// Pointer to the RGBA pixels, `vis_width()`·`vis_height()` u32.
#[no_mangle]
pub extern "C" fn vis_frame(now: f64, w: i32, h: i32, css: f64) -> *const u32 {
    let p = unsafe { &*(&raw const PLAYER) };
    vis().frame(now, w, h, css, p)
}

/// Draws a single view filling the whole frame, without bar and panel (see
/// [`Vis::view_frame`]).
///
/// # Arguments
/// * `view` - 0 oscilloscope, 1 Mandelbrot, 2 spectrum, 3 cube, 4 voxel landscape, 5 lake,
///   6 demo scene
/// * `now` - time in ms (performance.now())
/// * `w` - wanted width in pixels
/// * `h` - wanted height; clamped to the buffer, see [`vis_width`]/[`vis_height`]
/// * `css` - framebuffer pixels per CSS pixel
///
/// # Returns
/// Pointer to the RGBA pixels, `vis_width()`·`vis_height()` u32.
#[no_mangle]
pub extern "C" fn vis_view(view: u32, now: f64, w: i32, h: i32, css: f64) -> *const u32 {
    let p = unsafe { &*(&raw const PLAYER) };
    vis().view_frame(view, now, w, h, css, p)
}

/// Smooth surface or blocky voxels in the voxel views.
///
/// # Arguments
/// * `on` - 1 smooth (the default), 0 blocky
#[no_mangle]
pub extern "C" fn vis_set_fine(on: i32) {
    vis().set_fine(on != 0)
}

/// The view the renderer currently shows (numbers as in [`vis_view`]).
///
/// # Returns
/// 0 oscilloscope, 1 Mandelbrot, 2 spectrum, 3 cube, 4 voxel landscape, 5 lake, 6 demo scene
#[no_mangle]
pub extern "C" fn vis_current_view() -> u32 {
    vis().view
}

/// Views the view button (and the V key) steps through; the oscilloscope always stays in.
///
/// # Arguments
/// * `mask` - bit `v` set: view `v` is included (numbers as in [`vis_view`]); all are
///   included by default
#[no_mangle]
pub extern "C" fn vis_set_views(mask: u32) {
    vis().set_views(mask)
}

/// Width of the last frame.
///
/// # Returns
/// Pixels, after clamping.
#[no_mangle]
pub extern "C" fn vis_width() -> i32 {
    vis().width()
}

/// Height of the last frame.
///
/// # Returns
/// Pixels, after clamping.
#[no_mangle]
pub extern "C" fn vis_height() -> i32 {
    vis().height()
}

/// Pointer press.
///
/// # Arguments
/// * `x` - column in framebuffer pixels
/// * `y` - row in framebuffer pixels
///
/// # Returns
/// The action for JS: 0 none, 1 play/pause, 2 stop, 3 open file, 4 fullscreen, 5 seek to
/// `vis_action_arg()`, 7 loop on/off.
#[no_mangle]
pub extern "C" fn vis_pointer(x: i32, y: i32) -> i32 {
    vis().pointer(x, y)
}

/// Key press.
///
/// # Arguments
/// * `code` - character code of the key
///
/// # Returns
/// The action for JS: 0 none, 1 play/pause, 2 stop, 3 open file, 4 fullscreen, 5 seek to
/// `vis_action_arg()`, 7 loop on/off.
#[no_mangle]
pub extern "C" fn vis_key(code: u32) -> i32 {
    vis().key(code)
}

/// Argument of the last action.
///
/// # Returns
/// Seek position for 5, 1/0 (on/off) for 4 and 7.
#[no_mangle]
pub extern "C" fn vis_action_arg() -> i32 {
    vis().action_arg()
}

/// Tells the renderer whether the song is playing.
///
/// # Arguments
/// * `on` - 1 playing, 0 paused or stopped
#[no_mangle]
pub extern "C" fn vis_set_playing(on: i32) {
    vis().set_playing(on != 0)
}

/// The browser's fullscreen state, e.g. after leaving it with Escape.
///
/// # Arguments
/// * `on` - 1 in fullscreen, 0 not
#[no_mangle]
pub extern "C" fn vis_set_fullscreen(on: i32) {
    vis().set_fullscreen(on != 0)
}

/// Song events.
///
/// # Arguments
/// * `code` - 1 = song ended, 2 = new song loaded
#[no_mangle]
pub extern "C" fn vis_event(code: i32) {
    vis().event(code)
}

// ---- Mandelbrot jobs for the Web Workers (SIMD kernel in kernel/mandel_wasm.rs)

/// Tells the renderer who computes the Mandelbrot rows.
///
/// # Arguments
/// * `n` - number of Web Workers; 0 = the renderer itself
#[no_mangle]
pub extern "C" fn vis_mb_workers(n: i32) {
    vis().mb_set_workers(n)
}

/// Hands out a new Mandelbrot job, once.
///
/// # Returns
/// Its generation, or 0 if there is none; parameters via [`vis_mb_param`].
#[no_mangle]
pub extern "C" fn vis_mb_take() -> u32 {
    vis().mb_take()
}

/// Generation of the job being rendered; results of older jobs are dropped.
///
/// # Returns
/// The generation.
#[no_mangle]
pub extern "C" fn vis_mb_gen() -> u32 {
    vis().mb_gen()
}

/// A parameter of the current job.
///
/// # Arguments
/// * `i` - 0 re0, 1 im0, 2 step, 3 width (multiple of 8), 4 height, 5 max iterations
///
/// # Returns
/// The value.
#[no_mangle]
pub extern "C" fn vis_mb_param(i: i32) -> f64 {
    vis().mb_param(i)
}

/// Where the workers' rows go.
///
/// # Returns
/// Pointer to width·height u32 palette indices.
#[no_mangle]
pub extern "C" fn vis_mb_back_ptr() -> *mut u32 {
    vis().mb_back_ptr()
}

/// Rows have been copied into the back buffer.
///
/// # Arguments
/// * `gen` - generation of the job
/// * `rows` - number of rows
#[no_mangle]
pub extern "C" fn vis_mb_rows(gen: u32, rows: i32) {
    vis().mb_rows(gen, rows)
}
