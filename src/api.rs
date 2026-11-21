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
pub const MOD_CAPACITY: usize = 4 * 1024 * 1024;
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

/// Where JS copies the MOD file before `mp_load`.
#[no_mangle]
pub extern "C" fn mp_mod_buffer() -> *mut u8 {
    (&raw mut MOD_BUF) as *mut u8
}

/// Size of the buffer of `mp_mod_buffer` (bytes).
#[no_mangle]
pub extern "C" fn mp_mod_capacity() -> usize {
    MOD_CAPACITY
}

/// Parse the MOD previously copied into `mp_mod_buffer()` and start at position 0.
/// Returns the channel count (> 0) or a negative error code (-3: larger than the buffer;
/// others from [`Player::load`]).
#[no_mangle]
pub extern "C" fn mp_load(len: usize, sample_rate: f32) -> i32 {
    if len > MOD_CAPACITY {
        return -3;
    }
    player().load((&raw const MOD_BUF) as *const u8, len, sample_rate)
}

/// Render `frames` (max 4096) stereo frames; returns a pointer to 2*frames f32.
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

/// Planar per-channel output of the last `mp_render` (channels * frames).
#[no_mangle]
pub extern "C" fn mp_channels_ptr() -> *const f32 {
    (&raw const CHAN) as *const f32
}

/// Channel count of the loaded song.
#[no_mangle]
pub extern "C" fn mp_num_channels() -> u32 {
    player().channels as u32
}

/// Current song position (index into the order list).
#[no_mangle]
pub extern "C" fn mp_order() -> u32 {
    player().order_pos as u32
}

/// Current row (0..63).
#[no_mangle]
pub extern "C" fn mp_row() -> u32 {
    player().row as u32
}

/// Pattern at the current song position.
#[no_mangle]
pub extern "C" fn mp_pattern() -> u32 {
    player().pattern() as u32
}

/// Number of song positions.
#[no_mangle]
pub extern "C" fn mp_song_length() -> u32 {
    player().song_len as u32
}

/// Ticks per row.
#[no_mangle]
pub extern "C" fn mp_speed() -> u32 {
    player().speed
}

/// Tempo in BPM (125 = 50 ticks per second).
#[no_mangle]
pub extern "C" fn mp_tempo() -> u32 {
    player().tempo
}

/// How many times the song has reached its end.
#[no_mangle]
pub extern "C" fn mp_loops() -> u32 {
    player().loops
}

/// Jump to song position `order` (row 0).
#[no_mangle]
pub extern "C" fn mp_set_position(order: u32) {
    player().set_position(order as usize)
}

/// Stereo separation: 0 = mono, 1 = hard Amiga panning.
#[no_mangle]
pub extern "C" fn mp_set_separation(sep: f32) {
    player().separation = clamp32(sep, 0.0, 1.0);
}

/// Output gain, 0 ..= 2.
#[no_mangle]
pub extern "C" fn mp_set_gain(gain: f32) {
    player().gain = clamp32(gain, 0.0, 2.0);
}

/// FFT (Hann window) of the last `size` mono samples of the mix; `size` is a power
/// of two from 16 to 4096. Returns a pointer to size/2+1 power values
/// (amplitude², 1.0 = full-scale sine), or null for an unsupported size.
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

/// Where JS copies the next worklet message: channel scopes (1024 floats each, at most
/// 8 channels), then master L and R, then at vis_spec_offset() the 2049 spectrum values.
#[no_mangle]
pub extern "C" fn vis_slot_ptr() -> *mut f32 {
    vis().slot_ptr()
}

/// Offset of the spectrum within a slot (floats).
#[no_mangle]
pub extern "C" fn vis_spec_offset() -> u32 {
    crate::vis::SPEC_OFFSET as u32
}

/// The slot is filled: `t` is the time (performance.now() + output latency) when it is
/// heard; the rest is the replayer state at that moment (see [`Vis::commit`]).
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

/// Draw a frame of `w` x `h` pixels (clamped to the buffer, see vis_width/vis_height);
/// `css` = framebuffer pixels per CSS pixel. Returns a pointer to the RGBA pixels.
#[no_mangle]
pub extern "C" fn vis_frame(now: f64, w: i32, h: i32, css: f64) -> *const u32 {
    let p = unsafe { &*(&raw const PLAYER) };
    vis().frame(now, w, h, css, p)
}

/// Width of the last frame (after clamping).
#[no_mangle]
pub extern "C" fn vis_width() -> i32 {
    vis().width()
}

/// Height of the last frame (after clamping).
#[no_mangle]
pub extern "C" fn vis_height() -> i32 {
    vis().height()
}

/// Pointer press at framebuffer coordinates; returns an action for JS (0 none, 1 play/pause,
/// 2 stop, 3 open file, 4 fullscreen, 5 seek to vis_action_arg(), 7 loop on/off).
#[no_mangle]
pub extern "C" fn vis_pointer(x: i32, y: i32) -> i32 {
    vis().pointer(x, y)
}

/// Key press (character code); returns an action like `vis_pointer`.
#[no_mangle]
pub extern "C" fn vis_key(code: u32) -> i32 {
    vis().key(code)
}

/// Argument of the last action (seek position, loop or fullscreen state).
#[no_mangle]
pub extern "C" fn vis_action_arg() -> i32 {
    vis().action_arg()
}

/// Playing (1) or paused/stopped (0).
#[no_mangle]
pub extern "C" fn vis_set_playing(on: i32) {
    vis().set_playing(on != 0)
}

/// The browser's fullscreen state (1/0), e.g. after leaving it with Escape.
#[no_mangle]
pub extern "C" fn vis_set_fullscreen(on: i32) {
    vis().set_fullscreen(on != 0)
}

/// Song events: 1 = song ended, 2 = new song loaded.
#[no_mangle]
pub extern "C" fn vis_event(code: i32) {
    vis().event(code)
}

// ---- Mandelbrot jobs for the Web Workers (SIMD kernel in kernel/mandel_wasm.rs)

/// Tell the renderer that `n` workers render the Mandelbrot rows (0 = render itself).
#[no_mangle]
pub extern "C" fn vis_mb_workers(n: i32) {
    vis().mb_set_workers(n)
}

/// Generation of a new job to hand out, or 0. Parameters: vis_mb_param(0..5).
#[no_mangle]
pub extern "C" fn vis_mb_take() -> u32 {
    vis().mb_take()
}

/// Generation of the job currently being rendered (results of older jobs are dropped).
#[no_mangle]
pub extern "C" fn vis_mb_gen() -> u32 {
    vis().mb_gen()
}

/// 0 re0, 1 im0, 2 step, 3 width (multiple of 8), 4 height, 5 max iterations.
#[no_mangle]
pub extern "C" fn vis_mb_param(i: i32) -> f64 {
    vis().mb_param(i)
}

/// Where the workers' rows go: palette indices, width * height u32.
#[no_mangle]
pub extern "C" fn vis_mb_back_ptr() -> *mut u32 {
    vis().mb_back_ptr()
}

/// `rows` rows of job `gen` have been copied into the back buffer.
#[no_mangle]
pub extern "C" fn vis_mb_rows(gen: u32, rows: i32) {
    vis().mb_rows(gen, rows)
}
