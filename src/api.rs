// C ABI for JavaScript – no wasm-bindgen needed. Like replayer.rs, this file
// only uses the core language so it also builds with preview/minicore.rs.
//
// JS flow (inside the AudioWorklet):
//   buf = mp_mod_buffer(); copy MOD bytes there (max mp_mod_capacity());
//   mp_load(len, sampleRate)
//   per block: ptr = mp_render(frames)  -> interleaved stereo f32 (2*frames)
//              mp_channels_ptr()        -> planar per-channel f32 (channels*frames)

use crate::fft::{Fft, FFT_MAX};
use crate::replayer::{Player, MAX_CH};

pub const MOD_CAPACITY: usize = 4 * 1024 * 1024;
pub const MAX_FRAMES: usize = 4096;

static mut PLAYER: Player = Player::empty();
static mut MOD_BUF: [u8; MOD_CAPACITY] = [0; MOD_CAPACITY];
static mut MIX: [f32; 2 * MAX_FRAMES] = [0.0; 2 * MAX_FRAMES];
static mut CHAN: [f32; MAX_CH * MAX_FRAMES] = [0.0; MAX_CH * MAX_FRAMES];
// the last FFT_MAX mono samples of the mix, for the spectrum
static mut RING: [f32; FFT_MAX] = [0.0; FFT_MAX];
static mut RING_POS: usize = 0;
static mut FFT: Fft = Fft::empty();

fn player() -> &'static mut Player {
    unsafe { &mut *(&raw mut PLAYER) }
}

#[no_mangle]
pub extern "C" fn mp_mod_buffer() -> *mut u8 {
    (&raw mut MOD_BUF) as *mut u8
}

#[no_mangle]
pub extern "C" fn mp_mod_capacity() -> usize {
    MOD_CAPACITY
}

/// Parse the MOD previously copied into `mp_mod_buffer()`.
/// Returns the channel count (> 0) or a negative error code.
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
    unsafe {
        let ring = &mut *(&raw mut RING);
        let mut pos = RING_POS;
        let mut i = 0;
        while i < n {
            let l = *((mix as usize + 2 * i * 4) as *const f32);
            let r = *((mix as usize + (2 * i + 1) * 4) as *const f32);
            ring[pos] = (l + r) * 0.5;
            pos += 1;
            if pos == FFT_MAX {
                pos = 0;
            }
            i += 1;
        }
        RING_POS = pos;
    }
    mix as *const f32
}

/// Planar per-channel output of the last `mp_render` (channels * frames).
#[no_mangle]
pub extern "C" fn mp_channels_ptr() -> *const f32 {
    (&raw const CHAN) as *const f32
}

#[no_mangle]
pub extern "C" fn mp_num_channels() -> u32 {
    player().channels as u32
}

#[no_mangle]
pub extern "C" fn mp_order() -> u32 {
    player().order_pos as u32
}

#[no_mangle]
pub extern "C" fn mp_row() -> u32 {
    player().row as u32
}

#[no_mangle]
pub extern "C" fn mp_pattern() -> u32 {
    player().pattern() as u32
}

#[no_mangle]
pub extern "C" fn mp_song_length() -> u32 {
    player().song_len as u32
}

#[no_mangle]
pub extern "C" fn mp_speed() -> u32 {
    player().speed
}

#[no_mangle]
pub extern "C" fn mp_tempo() -> u32 {
    player().tempo
}

/// How many times the song has reached its end.
#[no_mangle]
pub extern "C" fn mp_loops() -> u32 {
    player().loops
}

#[no_mangle]
pub extern "C" fn mp_set_position(order: u32) {
    player().set_position(order as usize)
}

#[no_mangle]
pub extern "C" fn mp_set_separation(sep: f32) {
    let s = if sep < 0.0 {
        0.0
    } else if sep > 1.0 {
        1.0
    } else {
        sep
    };
    player().separation = s;
}

#[no_mangle]
pub extern "C" fn mp_set_gain(gain: f32) {
    let g = if gain < 0.0 {
        0.0
    } else if gain > 2.0 {
        2.0
    } else {
        gain
    };
    player().gain = g;
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
    f.analyse_ring((&raw const RING) as *const f32, FFT_MAX, unsafe {
        RING_POS
    });
    (&raw const f.power) as *const f32
}
