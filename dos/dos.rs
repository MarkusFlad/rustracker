//! DOS layer: runs the Rustracker on a PC with a Sound Blaster and VGA, in 32-bit protected
//! mode set up by RTRACK.COM (dos/stub.asm) and dos/entry.asm.
//!
//! * Sound: the song is mixed in the Sound Blaster's interrupt into the half of an
//!   auto-initialising DMA buffer that has just been played (SB16: 16-bit stereo 44.1 kHz,
//!   SB 2.0/Pro: 8-bit mono 22 kHz). Each block of 1024 frames also becomes a renderer slot
//!   (scopes, spectrum), exactly like the worklet's messages in the browser.
//! * Picture: the framebuffer renderer draws 320 x 200 RGBA frames; an adaptive 256-colour
//!   palette follows their colours (the 255 most frequent RGB555 colours, the others mapped
//!   to the nearest), and they are copied into VGA memory during the vertical retrace
//!   (mode 13h).
//! * Keyboard: polled from the keyboard controller (its interrupt stays masked).
//!
//! Memory is addressed through segments with the image's load address as base: physical
//! address `p` is pointer `p - base` (the 4 GB limit makes this wrap around correctly).

use crate::api::*;
use crate::vis::ACT_BENCH;

extern "C" {
    fn inb(port: u32) -> u32;
    fn outb(port: u32, value: u32);
    fn irq_enable();
    fn irq_disable();
    fn peek32(addr: *const u32) -> u32;
    fn poke32(addr: *mut u32, value: u32);
}

/// Frames per Sound Blaster block (half the DMA buffer) = one renderer slot.
const BLOCK: u32 = 1024;
/// Screen size of mode 13h.
const SW: i32 = 320;
const SH: i32 = 200;

/// What RTRACK.COM hands over (physical addresses).
#[repr(C)]
pub struct BootInfo {
    /// Physical address of the image (= segment base).
    pub base: u32,
    /// Bytes of memory from `base` on.
    pub mem_size: u32,
    /// The MOD file.
    pub mod_phys: u32,
    pub mod_len: u32,
    /// Conventional memory for the DMA buffer.
    pub dma_phys: u32,
    pub dma_len: u32,
    /// The value of the BLASTER environment variable (ASCIIZ), or 0.
    pub env_phys: u32,
    /// Conventional memory for messages that RTRACK.COM prints after returning to DOS.
    pub log_phys: u32,
    pub log_len: u32,
    /// The value of the RTRACK environment variable (ASCIIZ), or 0: `T<n>` quits after n
    /// seconds of playback (for automatic tests).
    pub opt_phys: u32,
    /// Written by dos/entry.asm on a CPU exception: the faulting EIP and the error code.
    pub exc_eip: u32,
    /// See `exc_eip`.
    pub exc_err: u32,
    /// The DOS command line: a length byte followed by the text (PSP offset 80h).
    pub cmd_phys: u32,
    /// Written here after a benchmark: offset and length of its table in the message
    /// buffer; RTRACK.COM also writes that part to BENCH.TXT.
    pub bench_off: u32,
    /// See `bench_off`.
    pub bench_len: u32,
}

/// Sound Blaster settings and state.
struct Sb {
    port: u32,
    irq: u32,
    dma8: u32,
    dma16: u32,
    sixteen: bool,
    rate: u32,
    /// Start of the DMA buffer (physical) and bytes per half.
    buf: u32,
    half_bytes: u32,
}

static mut BASE: u32 = 0;
static mut SB: Sb = Sb {
    port: 0x220,
    irq: 5,
    dma8: 1,
    dma16: 5,
    sixteen: false,
    rate: 22050,
    buf: 0,
    half_bytes: 0,
};
static mut LOG_P: u32 = 0;
static mut LOG_LEN: u32 = 0;
static mut LOG_N: u32 = 0;

// shared between the interrupt and the main loop
static mut N_IRQ: u32 = 0; // halves played
static mut NEXT_HALF: u32 = 0; // the half the next interrupt refills
static mut MIXED: u32 = 0; // blocks mixed so far
static mut PAUSED: u32 = 0;
static mut PEND_READY: u32 = 0;
static mut PENDING: [f32; 10 * 1024] = [0.0; 10 * 1024];
static mut PEND_META: [u32; 7] = [0; 7]; // order, row, pattern, speed, tempo, nch, t (ms)
static mut LAST_T: f64 = 0.0;
static mut QUIT_MS: f64 = 0.0; // > 0: quit at this playback time
static mut BENCH_SECS: f64 = 0.0; // > 0: start the benchmark with this time per scene
static mut BENCH_QUIT: bool = false; // quit when the benchmark has finished (RTRACK=B<n>)
static mut SPEC_TURN: u32 = 0;
// watchdog: frames finished by the main loop, and its state in the interrupt
static mut FRAMES_DONE: u32 = 0;
static mut WD_FRAME: u32 = 0;
static mut WD_IRQ: u32 = 0;
static mut SPEC_PTR: usize = 0;

// picture: an adaptive 256-colour palette over the colours of RGB555 (see `update_palette`)
static mut BACK: [u8; 64000] = [0; 64000];
/// Colour (RGB555) in each palette slot, or `FREE`.
static mut PAL15: [u32; 256] = [FREE; 256];
/// Slots whose DAC entry still has to be written.
static mut DIRTY: [u8; 256] = [0; 256];
/// Slot + 1 of each RGB555 colour that has a slot of its own, else 0.
static mut SLOT_OF: [u32; 32768] = [0; 32768];
/// Cached slot for each RGB555 colour, `UNKNOWN` until looked up (cleared when the palette
/// changes). One byte per colour keeps the table small enough for the Pentium's cache.
static mut LUT: [u8; 32768] = [0; 32768];
/// Marks an empty `LUT` entry; therefore slot 255 is never used.
const UNKNOWN: u8 = 255;
const SLOTS: usize = 255;
/// Pixels per RGB555 colour in the frame being analysed, and the colours that occur.
static mut COUNT: [u32; 32768] = [0; 32768];
static mut USED: [u32; 32768] = [0; 32768];
/// Colours chosen for the palette (marked with `MARK_ID` in `MARK`).
static mut MARK: [u32; 32768] = [0; 32768];
static mut MARK_ID: u32 = 0;
const FREE: u32 = 0xFFFF_FFFF;
/// Frames between two palette analyses.
const PALETTE_EVERY: u32 = 8;

/// Pointer for a physical address.
fn phys(p: u32) -> usize {
    unsafe { (p - BASE) as usize }
}

// ------------------------------------------------------------------ log

fn log_byte(b: u32) {
    unsafe {
        if LOG_P != 0 && LOG_N + 1 < LOG_LEN {
            *((phys(LOG_P) + LOG_N as usize) as *mut u8) = b as u8;
            LOG_N += 1;
            *((phys(LOG_P) + LOG_N as usize) as *mut u8) = 0;
        }
    }
}

fn log<const N: usize>(s: &[u8; N]) {
    let mut i = 0;
    while i < N {
        log_byte(s[i] as u32);
        i += 1;
    }
}

fn log_hex(v: u32, digits: u32) {
    let mut i = digits;
    while i > 0 {
        i -= 1;
        let d = (v >> (i * 4)) & 15;
        log_byte(if d < 10 { 48 + d } else { 55 + d });
    }
}

fn log_dec(v: u32) {
    let mut d = [0u32; 10];
    let mut n = 0;
    let mut x = v;
    while n == 0 || x > 0 {
        d[n] = x % 10;
        x /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        log_byte(48 + d[n]);
    }
}

// ------------------------------------------------------------------ entry points

/// Called by dos/entry.asm in protected mode. Returns the exit code for RTRACK.COM.
#[no_mangle]
pub extern "C" fn rust_main(base: u32, info_phys: u32) -> u32 {
    unsafe {
        BASE = base;
        let bi = &*(phys(info_phys) as *const BootInfo);
        LOG_P = bi.log_phys;
        LOG_LEN = bi.log_len;
        log(b"RUSTRACKER for DOS\r\n");
        log(b"Program at ");
        log_hex(base, 8);
        log(b", song at ");
        log_hex(bi.mod_phys, 8);
        log(b" (");
        log_dec(bi.mod_len);
        log(b" bytes), DMA memory at ");
        log_hex(bi.dma_phys, 8);
        log(b"\r\n");
        if bi.env_phys != 0 {
            parse_blaster(phys(bi.env_phys));
        }
        if !parse_options(phys(bi.cmd_phys)) {
            return 0;
        }
        if bi.opt_phys != 0 {
            let o = phys(bi.opt_phys);
            let c = *(o as *const u8) as u32;
            if c == 84 || c == 116 {
                QUIT_MS = parse_num(o + 1, 10) as f64 * 1000.0;
            } else if c == 66 || c == 98 {
                // B<n>: benchmark with n seconds per scene, then quit (for automatic tests)
                BENCH_SECS = parse_num(o + 1, 10) as f64;
                if BENCH_SECS < 1.0 {
                    BENCH_SECS = 15.0;
                }
                BENCH_QUIT = true;
            }
        }
        if !sb_reset() {
            log(b"No Sound Blaster found at port ");
            log_hex(SB.port, 3);
            log(b" (check the BLASTER variable).\r\n");
            return 2;
        }
        let ver = sb_version();
        if ver < 0x200 {
            log(b"A Sound Blaster with DSP 2.0 or newer is required.\r\n");
            return 2;
        }
        SB.sixteen = ver >= 0x400;
        if SB.sixteen {
            sb16_config();
        }
        SB.rate = if SB.sixteen {
            44100
        } else {
            1_000_000 / (256 - (256 - 1_000_000 / 22050))
        };
        let n = mp_load_ptr(
            phys(bi.mod_phys) as *const u8,
            bi.mod_len as usize,
            SB.rate as f32,
        );
        if n <= 0 {
            log(b"Could not read the MOD file.\r\n");
            return 3;
        }
        if !setup_dma_buffer(bi.dma_phys, bi.dma_len) {
            log(b"No suitable DMA buffer.\r\n");
            return 4;
        }
        log(b"Sound Blaster ");
        if SB.sixteen {
            log(b"16");
        } else {
            log(b"Pro");
        }
        log(b" at ");
        log_hex(SB.port, 3);
        log(b", IRQ ");
        log_dec(SB.irq);
        log(b", DMA ");
        log_dec(if SB.sixteen { SB.dma16 } else { SB.dma8 });
        log(b", DSP ");
        log_dec(ver >> 8);
        log(b".");
        log_dec(ver & 255);
        log(b", ");
        log_dec(SB.rate);
        log(b" Hz, buffer at ");
        log_hex(SB.buf, 8);
        log(b"\r\n");
        vis_event(2);
        vis_set_playing(1);
        clear_lut();
        log(b"Playback started\r\n");
        if BENCH_SECS > 0.0 {
            vis_bench_start(BENCH_SECS);
        }
        sb_start();
        let ok = main_loop();
        sb_stop();
        if !ok {
            no_interrupt_help();
            return 5;
        }
        log(b"Stopped after ");
        log_dec(peek32(&raw const N_IRQ));
        log(b" blocks (Sound Blaster interrupts).\r\n");
        log_bench(info_phys);
        log(b"Bye!\r\n");
        0
    }
}

/// Appends the table of the last benchmark (if any) to the messages and tells RTRACK.COM
/// where it is, so that it also writes it to BENCH.TXT.
unsafe fn log_bench(info_phys: u32) {
    let n = vis_bench_len() as usize;
    if n == 0 {
        return;
    }
    log(b"\r\n");
    let start = LOG_N;
    let p = vis_bench_text() as usize;
    let mut i = 0;
    while i < n {
        log_byte(*((p + i) as *const u8) as u32);
        i += 1;
    }
    let bi = &mut *(phys(info_phys) as *mut BootInfo);
    bi.bench_off = start;
    bi.bench_len = LOG_N - start;
    log(b"(also written to BENCH.TXT)\r\n\r\n");
}

/// A hardware interrupt `irq` (0..15); dos/entry.asm saves the FPU state around it. The Sound
/// Blaster's interrupt mixes the next block; every interrupt is acknowledged at the PIC.
#[no_mangle]
pub extern "C" fn rust_irq(irq: u32) -> u32 {
    unsafe {
        let mut hang = false;
        if irq == SB.irq {
            inb(SB.port + if SB.sixteen { 0x0F } else { 0x0E }); // acknowledge at the DSP
            let h = NEXT_HALF;
            fill_half(h);
            NEXT_HALF = h ^ 1;
            N_IRQ += 1;
            hang = watchdog();
        }
        if irq >= 8 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);
        if hang {
            sb_stop();
            log(b"The main loop has not finished a frame for 30 seconds.\r\n");
            return 1; // dos/entry.asm returns to DOS and reports where the loop was
        }
        0
    }
}

/// Watchdog: true when the main loop has not finished a frame for 30 seconds of playback
/// (the slowest views need a few seconds per frame on a Pentium 133).
unsafe fn watchdog() -> bool {
    let f = FRAMES_DONE;
    if f != WD_FRAME {
        WD_FRAME = f;
        WD_IRQ = N_IRQ;
        return false;
    }
    (N_IRQ - WD_IRQ) as f64 * BLOCK as f64 > 30.0 * SB.rate as f64
}

// ------------------------------------------------------------------ main loop

/// Keyboard, renderer slots and frames until Escape.
/// Returns false if the Sound Blaster never sent an interrupt.
unsafe fn main_loop() -> bool {
    let mut e0 = false;
    let mut first = true;
    let mut frame = 0u32;
    loop {
        let k = read_key();
        if k >= 0 {
            let code = k as u32;
            if code == 0xE0 {
                e0 = true;
                continue;
            }
            let ext = e0;
            e0 = false;
            if code & 0x80 == 0 && handle_key(code, ext) {
                return true;
            }
        }
        if peek32(&raw const PEND_READY) != 0 {
            commit_pending();
        }
        let now = audio_ms();
        if QUIT_MS > 0.0 && now >= QUIT_MS {
            return true;
        }
        let bench = vis_bench_state();
        if bench == 2 && BENCH_QUIT {
            return true;
        }
        // a frame takes at least one vertical retrace (1/70 s): after 100 frames without
        // a single interrupt, interrupt or DMA channel of the card are not what we use
        if frame == 100 && peek32(&raw const N_IRQ) == 0 {
            return false;
        }
        let fb = vis_frame(now, SW, SH, 0.5) as usize;
        if first {
            vis_set_fine(0); // coarse voxels (the renderer sets its defaults on the first frame)
            first = false;
        }
        if frame % PALETTE_EVERY == 0 {
            update_palette(fb);
        }
        frame += 1;
        convert_frame(fb);
        if bench != 1 {
            wait_retrace(); // the benchmark measures without waiting for the retrace (70 Hz)
        }
        write_dac();
        show_frame();
        poke32(&raw mut FRAMES_DONE, frame);
    }
}

/// A scan code from the keyboard controller, or -1.
unsafe fn read_key() -> i32 {
    let st = inb(0x64);
    if st & 1 == 0 {
        return -1;
    }
    let code = inb(0x60) as i32;
    if st & 0x20 != 0 {
        return -1; // mouse data
    }
    code
}

/// Carries out a key (set-1 make code); true for Escape.
unsafe fn handle_key(code: u32, ext: bool) -> bool {
    if code == 0x01 {
        return true;
    }
    if vis_bench_state() == 2 {
        vis_bench_stop(); // any key closes the benchmark result
        return false;
    }
    if code == 0x30 {
        // b: start the benchmark from the beginning of the song, or cancel it
        if vis_key(98) == ACT_BENCH {
            irq_disable();
            mp_set_position(0);
            irq_enable();
            poke32(&raw mut PAUSED, 0);
            vis_set_playing(1);
            vis_event(2);
        }
        return false;
    }
    if ext && (code == 0x4D || code == 0x4B) {
        irq_disable();
        let o = mp_order();
        let n = mp_song_length();
        let next = if code == 0x4D {
            if o + 1 < n {
                o + 1
            } else {
                0
            }
        } else if o > 0 {
            o - 1
        } else {
            0
        };
        mp_set_position(next);
        irq_enable();
        vis_event(2);
        return false;
    }
    if code == 0x39 {
        let p = peek32(&raw const PAUSED) ^ 1;
        poke32(&raw mut PAUSED, p);
        vis_set_playing(if p != 0 { 0 } else { 1 });
    } else if code == 0x1F {
        poke32(&raw mut PAUSED, 1);
        irq_disable();
        mp_set_position(0);
        irq_enable();
        vis_set_playing(0);
        vis_event(2);
    } else if code == 0x2F {
        vis_key(118); // v: next view
    } else if code == 0x04 {
        vis_key(51); // 3: spectrum 2D/3D
    } else if code == 0x19 {
        vis_key(112); // p: palette
    } else if code == 0x13 {
        vis_key(114); // r: voxels coarse/fine
    } else if code == 0x21 {
        vis_key(102); // f: view only (no bar and panel)
    }
    false
}

/// Copies the block captured in the interrupt into a renderer slot.
unsafe fn commit_pending() {
    irq_disable();
    let nch = PEND_META[5] as usize;
    let slot = vis_slot_ptr() as usize;
    let mut i = 0;
    while i < (nch + 2) * 1024 {
        *((slot + i * 4) as *mut f32) = PENDING[i];
        i += 1;
    }
    let mut meta = [0u32; 7];
    i = 0;
    while i < 7 {
        meta[i] = PEND_META[i];
        i += 1;
    }
    poke32(&raw mut PEND_READY, 0);
    irq_enable();
    // the 4096-point FFT costs a lot on a Pentium: only for the views that show the spectrum
    // (spectrum, cube, lake), and there a new one every other slot; the others get silence
    let off = vis_spec_offset() as usize;
    let view = vis_current_view();
    if view == 2 || view == 3 || view == 5 {
        SPEC_TURN = SPEC_TURN ^ 1;
        if SPEC_TURN != 0 || SPEC_PTR == 0 {
            SPEC_PTR = mp_spectrum(4096) as usize;
        }
        let sp = SPEC_PTR;
        i = 0;
        while i < 2049 {
            *((slot + (off + i) * 4) as *mut f32) = *((sp + i * 4) as *const f32);
            i += 1;
        }
    } else {
        i = 0;
        while i < 2049 {
            *((slot + (off + i) * 4) as *mut f32) = 0.0;
            i += 1;
        }
    }
    vis_commit(
        meta[6] as f64,
        meta[0],
        meta[1],
        meta[2],
        meta[3],
        meta[4],
        meta[5],
        SB.rate as f64,
    );
}

/// Time of the audio being heard (ms since the start of playback), from the number of
/// halves played and the DMA position; never runs backwards.
unsafe fn audio_ms() -> f64 {
    let n = peek32(&raw const N_IRQ);
    let pos = dma_position() % BLOCK;
    let t = (n * BLOCK + pos) as f64 * 1000.0 / SB.rate as f64;
    if t > LAST_T {
        LAST_T = t;
    }
    LAST_T
}

// ------------------------------------------------------------------ sound

/// Mixes the next block into half `h` of the DMA buffer and captures it for the renderer.
unsafe fn fill_half(h: u32) {
    let dst = phys(SB.buf + h * SB.half_bytes);
    if peek32(&raw const PAUSED) != 0 {
        let fill = if SB.sixteen { 0 } else { 0x80 };
        let mut i = 0;
        while i < SB.half_bytes as usize {
            *((dst + i) as *mut u8) = fill;
            i += 1;
        }
        return;
    }
    let mix = mp_render(BLOCK as usize) as usize;
    let mut i = 0;
    while i < BLOCK as usize {
        let l = *((mix + i * 8) as *const f32);
        let r = *((mix + i * 8 + 4) as *const f32);
        if SB.sixteen {
            *((dst + i * 4) as *mut i16) = to_i16(l);
            *((dst + i * 4 + 2) as *mut i16) = to_i16(r);
        } else {
            let m = (l + r) * 0.5;
            *((dst + i) as *mut u8) = (to_i16(m) as i32 / 256 + 128) as u8;
        }
        i += 1;
    }
    capture(mix);
    MIXED += 1;
}

fn to_i16(v: f32) -> i16 {
    let x = v * 32767.0;
    (if x > 32767.0 {
        32767.0
    } else if x < -32768.0 {
        -32768.0
    } else {
        x
    }) as i16
}

/// The block's channel scopes and master signal (like a worklet message).
unsafe fn capture(mix: usize) {
    let nch = mp_num_channels() as usize;
    let n = if nch > 8 { 8 } else { nch };
    let ch = mp_channels_ptr() as usize;
    let mut i = 0;
    while i < n * 1024 {
        PENDING[i] = *((ch + i * 4) as *const f32);
        i += 1;
    }
    i = 0;
    while i < 1024 {
        PENDING[n * 1024 + i] = *((mix + i * 8) as *const f32);
        PENDING[(n + 1) * 1024 + i] = *((mix + i * 8 + 4) as *const f32);
        i += 1;
    }
    PEND_META = [
        mp_order(),
        mp_row(),
        mp_pattern(),
        mp_speed(),
        mp_tempo(),
        n as u32,
        (MIXED as f64 * BLOCK as f64 * 1000.0 / SB.rate as f64) as u32,
    ];
    poke32(&raw mut PEND_READY, 1);
}

/// SB16: the card's own IRQ and DMA settings (mixer registers 80h and 81h) take precedence
/// over the BLASTER variable, which is often missing or wrong outside Windows. Without a
/// 16-bit DMA channel the card plays like a Sound Blaster Pro (8 bit).
unsafe fn sb16_config() {
    outb(SB.port + 4, 0x80);
    let irq_bits = inb(SB.port + 5) & 15;
    outb(SB.port + 4, 0x81);
    let dma_bits = inb(SB.port + 5);
    let irq = if irq_bits == 1 {
        9
    } else if irq_bits == 2 {
        5
    } else if irq_bits == 4 {
        7
    } else if irq_bits == 8 {
        10
    } else {
        0
    };
    if irq != 0 && irq != SB.irq {
        log(b"The card uses IRQ ");
        log_dec(irq);
        log(b" (BLASTER: ");
        log_dec(SB.irq);
        log(b")\r\n");
        SB.irq = irq;
    }
    let dma8 = if dma_bits & 2 != 0 {
        1
    } else if dma_bits & 1 != 0 {
        0
    } else if dma_bits & 8 != 0 {
        3
    } else {
        SB.dma8
    };
    if dma8 != SB.dma8 {
        log(b"The card uses DMA ");
        log_dec(dma8);
        log(b" (BLASTER: ");
        log_dec(SB.dma8);
        log(b")\r\n");
        SB.dma8 = dma8;
    }
    let dma16 = if dma_bits & 0x20 != 0 {
        5
    } else if dma_bits & 0x40 != 0 {
        6
    } else if dma_bits & 0x80 != 0 {
        7
    } else {
        0
    };
    if dma16 == 0 {
        if dma_bits & 0x0B != 0 {
            log(b"No 16-bit DMA channel set: 8-bit mono like a Sound Blaster Pro\r\n");
            SB.sixteen = false;
        }
    } else if dma16 != SB.dma16 {
        log(b"The card uses 16-bit DMA ");
        log_dec(dma16);
        log(b" (BLASTER: H");
        log_dec(SB.dma16);
        log(b")\r\n");
        SB.dma16 = dma16;
    }
}

/// Explains a missing interrupt: whether the DMA transfer ran at all, and what to check.
unsafe fn no_interrupt_help() {
    log(b"The Sound Blaster has not sent a single interrupt (IRQ ");
    log_dec(SB.irq);
    log(b", DMA ");
    log_dec(if SB.sixteen { SB.dma16 } else { SB.dma8 });
    log(b").\r\nPlease compare the BLASTER variable with the settings of the card, e.g.\r\n");
    log(b"SET BLASTER=A220 I5 D1 H5 T6. Under plain DOS, plug-and-play cards must first be\r\n");
    log(b"set up by their driver (Creative: CTCM or DIAGNOSE /S in AUTOEXEC.BAT).\r\n");
}

/// Views without a command-line option: all but Mandelbrot zoom (1), cube (3) and voxel
/// landscape (4), which are slow on a Pentium 133 and in DOSBox.
const DEFAULT_VIEWS: u32 = 0x65; // 0b110_0101

/// Reads the options of the command line `p` (length byte, then the text): /M adds the
/// Mandelbrot zoom, /W the cube, /V the voxel landscape, /? only prints the help. Words without / or -
/// are the song (RTRACK.COM loads it). Returns false when the program should not start.
unsafe fn parse_options(p: usize) -> bool {
    let len = *(p as *const u8) as usize;
    let mut views = DEFAULT_VIEWS;
    let mut i = 1;
    while i <= len {
        let c = *((p + i) as *const u8) as u32;
        let word_start = i == 1 || *((p + i - 1) as *const u8) as u32 <= 32;
        if word_start && (c == 47 || c == 45) && i < len {
            let o = *((p + i + 1) as *const u8) as u32 & 0xDF; // upper case
            if o == 77 {
                views = views | (1 << 1); // M
            } else if o == 86 {
                views = views | (1 << 4); // V
            } else if o == 87 {
                views = views | (1 << 3); // W
            } else if o == 66 {
                BENCH_SECS = 15.0; // B
            } else {
                log(b"RTRACK [options] [FILE.MOD]\r\n");
                log(b"  /M  add the Mandelbrot zoom (slow)\r\n");
                log(b"  /W  add the cube (slow)\r\n");
                log(b"  /V  add the voxel landscape (very slow)\r\n");
                log(b"  /B  benchmark: every view for 15 s, then a table (also in BENCH.TXT)\r\n");
                log(b"  /?  this help\r\n");
                return false;
            }
        }
        i += 1;
    }
    vis_set_views(views);
    true
}

/// BLASTER=A220 I5 D1 H5 T6: port, interrupt, 8-bit and 16-bit DMA channels.
unsafe fn parse_blaster(p: usize) {
    let mut i = 0;
    while i < 64 {
        let c = *((p + i) as *const u8) as u32;
        if c == 0 {
            return;
        }
        let up = if c >= 97 { c - 32 } else { c };
        if up == 65 {
            SB.port = parse_num(p + i + 1, 16);
        } else if up == 73 {
            SB.irq = parse_num(p + i + 1, 10);
            if SB.irq == 2 {
                SB.irq = 9; // IRQ 2 of the XT bus arrives as IRQ 9 on an AT
            }
        } else if up == 68 {
            SB.dma8 = parse_num(p + i + 1, 10);
        } else if up == 72 {
            SB.dma16 = parse_num(p + i + 1, 10);
        }
        i += 1;
    }
}

unsafe fn parse_num(p: usize, radix: u32) -> u32 {
    let mut v = 0;
    let mut i = 0;
    while i < 8 {
        let c = *((p + i) as *const u8) as u32;
        let d = if c >= 48 && c <= 57 {
            c - 48
        } else if radix == 16 && c >= 65 && c <= 70 {
            c - 55
        } else if radix == 16 && c >= 97 && c <= 102 {
            c - 87
        } else {
            return v;
        };
        v = v * radix + d;
        i += 1;
    }
    v
}

unsafe fn delay() {
    let mut i = 0;
    while i < 8 {
        inb(0x80);
        i += 1;
    }
}

unsafe fn dsp_write(v: u32) {
    let mut n = 0;
    while inb(SB.port + 0x0C) & 0x80 != 0 && n < 100_000 {
        n += 1;
    }
    outb(SB.port + 0x0C, v);
}

unsafe fn dsp_read() -> u32 {
    let mut n = 0;
    while inb(SB.port + 0x0E) & 0x80 == 0 && n < 100_000 {
        n += 1;
    }
    inb(SB.port + 0x0A)
}

/// Resets the DSP; true if it answers with 0xAA.
unsafe fn sb_reset() -> bool {
    outb(SB.port + 6, 1);
    delay();
    outb(SB.port + 6, 0);
    let mut n = 0;
    while n < 1000 {
        if inb(SB.port + 0x0E) & 0x80 != 0 && inb(SB.port + 0x0A) == 0xAA {
            return true;
        }
        delay();
        n += 1;
    }
    false
}

/// DSP version, major in the high byte.
unsafe fn sb_version() -> u32 {
    dsp_write(0xE1);
    let major = dsp_read();
    let minor = dsp_read();
    (major << 8) | minor
}

/// Places the DMA buffer (two halves) in the conventional memory block so that it does not
/// cross a 64 KB page (128 KB and word aligned for 16-bit DMA).
unsafe fn setup_dma_buffer(p: u32, len: u32) -> bool {
    SB.half_bytes = if SB.sixteen { BLOCK * 4 } else { BLOCK };
    let need = 2 * SB.half_bytes;
    let mut b = (p + 15) & 0xFFFF_FFF0;
    while b + need <= p + len {
        let page = if SB.sixteen { 0x1_FFFF } else { 0xFFFF };
        if (b | page) == ((b + need - 1) | page) {
            SB.buf = b;
            return true;
        }
        b += 16;
    }
    false
}

/// Fills both halves, programs DMA and DSP, unmasks the interrupt and starts playback.
unsafe fn sb_start() {
    fill_half(0);
    fill_half(1);
    NEXT_HALF = 0;
    let bytes = 2 * SB.half_bytes;
    if SB.sixteen {
        let c = SB.dma16 - 4;
        outb(0xD4, 4 | c);
        outb(0xD8, 0);
        outb(0xD6, 0x58 | c);
        let addr_port = 0xC0 + c * 4;
        let words = SB.buf >> 1;
        outb(addr_port, words & 0xFF);
        outb(addr_port, (words >> 8) & 0xFF);
        outb(addr_port + 2, (bytes / 2 - 1) & 0xFF);
        outb(addr_port + 2, ((bytes / 2 - 1) >> 8) & 0xFF);
        outb(dma16_page(SB.dma16), (SB.buf >> 16) & 0xFE);
        outb(0xD4, c);
    } else {
        let c = SB.dma8;
        outb(0x0A, 4 | c);
        outb(0x0C, 0);
        outb(0x0B, 0x58 | c);
        outb(c * 2, SB.buf & 0xFF);
        outb(c * 2, (SB.buf >> 8) & 0xFF);
        outb(c * 2 + 1, (bytes - 1) & 0xFF);
        outb(c * 2 + 1, ((bytes - 1) >> 8) & 0xFF);
        outb(dma8_page(c), (SB.buf >> 16) & 0xFF);
        outb(0x0A, c);
    }
    unmask_irq(SB.irq);
    irq_enable();
    dsp_write(0xD1); // speaker on
    if SB.sixteen {
        dsp_write(0x41);
        dsp_write(SB.rate >> 8);
        dsp_write(SB.rate & 0xFF);
        let len = BLOCK * 2 - 1; // samples per block (stereo)
        dsp_write(0xB6);
        dsp_write(0x30);
        dsp_write(len & 0xFF);
        dsp_write(len >> 8);
    } else {
        dsp_write(0x40);
        dsp_write(256 - 1_000_000 / 22050);
        let len = BLOCK - 1;
        dsp_write(0x48);
        dsp_write(len & 0xFF);
        dsp_write(len >> 8);
        dsp_write(0x1C);
    }
}

/// Stops playback, masks the interrupt and resets the DSP.
unsafe fn sb_stop() {
    if SB.sixteen {
        dsp_write(0xD5);
        dsp_write(0xD9);
    } else {
        dsp_write(0xD0);
        dsp_write(0xDA);
    }
    dsp_write(0xD3); // speaker off
    irq_disable();
    mask_irq(SB.irq);
    if SB.sixteen {
        outb(0xD4, 4 | (SB.dma16 - 4));
    } else {
        outb(0x0A, 4 | SB.dma8);
    }
    sb_reset();
}

fn dma8_page(c: u32) -> u32 {
    if c == 0 {
        0x87
    } else if c == 1 {
        0x83
    } else if c == 2 {
        0x81
    } else {
        0x82
    }
}

fn dma16_page(c: u32) -> u32 {
    if c == 5 {
        0x8B
    } else if c == 6 {
        0x89
    } else {
        0x8A
    }
}

/// Position of the DMA transfer within the whole buffer, in frames.
unsafe fn dma_position() -> u32 {
    let total_frames = 2 * BLOCK;
    if SB.sixteen {
        let port = 0xC2 + (SB.dma16 - 4) * 4;
        outb(0xD8, 0);
        let lo = inb(port);
        let hi = inb(port);
        let left = ((hi << 8) | lo) + 1; // words still to transfer
        let done = (2 * SB.half_bytes) / 2 - (left & 0xFFFF);
        (done / 2) % total_frames
    } else {
        let port = SB.dma8 * 2 + 1;
        outb(0x0C, 0);
        let lo = inb(port);
        let hi = inb(port);
        let left = ((hi << 8) | lo) + 1;
        let done = 2 * SB.half_bytes - (left & 0xFFFF);
        done % total_frames
    }
}

unsafe fn unmask_irq(irq: u32) {
    if irq < 8 {
        outb(0x21, inb(0x21) & (0xFF ^ (1 << irq)));
    } else {
        outb(0xA1, inb(0xA1) & (0xFF ^ (1 << (irq - 8))));
        outb(0x21, inb(0x21) & (0xFF ^ 4));
    }
}

unsafe fn mask_irq(irq: u32) {
    if irq < 8 {
        outb(0x21, inb(0x21) | (1 << irq));
    } else {
        outb(0xA1, inb(0xA1) | (1 << (irq - 8)));
    }
}

// ------------------------------------------------------------------ picture
//
// VGA mode 13h shows 256 colours at a time, each chosen from 262 144 (6 bits per channel).
// Instead of a fixed palette with dithering, the palette follows the picture: every few
// frames the colours of the frame are counted (in RGB555, 32 768 possible colours) and the
// 255 most frequent ones get a slot (slot 255 marks empty cache entries). Colours that keep their place keep their slot, so only
// new colours change DAC entries (written during the vertical retrace). Pixels whose colour
// has no slot get the nearest slot colour; that search is cached per colour.

/// RGB555 key of an RGBA pixel.
#[inline(always)]
fn key15(c: u32) -> usize {
    (((c & 0xF8) << 7) | ((c >> 6) & 0x3E0) | ((c >> 19) & 0x1F)) as usize
}

/// Counts the colours of frame `fb` and gives the most frequent 256 a palette slot.
unsafe fn update_palette(fb: usize) {
    // histogram, and the list of colours that occur
    let mut n = 0usize;
    let mut i = 0;
    while i < 64000 {
        let k = key15(*((fb + i * 4) as *const u32));
        let cnt = COUNT[k];
        if cnt == 0 {
            USED[n] = k as u32;
            n += 1;
        }
        COUNT[k] = cnt + 1;
        i += 1;
    }
    // the 255 most frequent colours: whole classes of log2(count) from the top
    let mut classes = [0u32; 17];
    i = 0;
    while i < n {
        classes[log2_class(COUNT[USED[i] as usize])] += 1;
        i += 1;
    }
    let mut cut = 16usize; // colours of class > cut all fit
    let mut taken = 0u32;
    while cut > 0 && taken + classes[cut] <= SLOTS as u32 {
        taken += classes[cut];
        cut -= 1;
    }
    let mut room = SLOTS as u32 - taken; // places left for colours of class == cut
    MARK_ID = if MARK_ID == 0xFFFF_FFFF {
        1
    } else {
        MARK_ID + 1
    };
    if MARK_ID == 1 {
        let mut j = 0;
        while j < 32768 {
            MARK[j] = 0;
            j += 1;
        }
    }
    i = 0;
    while i < n {
        let k = USED[i] as usize;
        let cl = log2_class(COUNT[k]);
        if cl > cut || (cl == cut && room > 0) {
            if cl == cut {
                room -= 1;
            }
            MARK[k] = MARK_ID;
        }
        i += 1;
    }
    // free the slots of colours that dropped out, then give new colours a free slot
    let mut s = 0;
    while s < 256 {
        let c = PAL15[s];
        if c != FREE && MARK[c as usize] != MARK_ID {
            SLOT_OF[c as usize] = 0;
            PAL15[s] = FREE;
        }
        s += 1;
    }
    let mut changed = false;
    let mut free = 0;
    i = 0;
    while i < n {
        let k = USED[i] as usize;
        if MARK[k] == MARK_ID && SLOT_OF[k] == 0 {
            while free < SLOTS && PAL15[free] != FREE {
                free += 1;
            }
            if free < SLOTS {
                PAL15[free] = k as u32;
                SLOT_OF[k] = free as u32 + 1;
                DIRTY[free] = 1;
                changed = true;
            }
        }
        COUNT[k] = 0;
        i += 1;
    }
    if changed {
        clear_lut();
    }
}

/// Empties the colour cache (all entries `UNKNOWN`).
unsafe fn clear_lut() {
    let p = (&raw mut LUT) as usize;
    let mut j = 0;
    while j < 32768 / 4 {
        *((p + j * 4) as *mut u32) = 0xFFFF_FFFF;
        j += 1;
    }
}

/// 0 for a count of 1, 1 for 2..3, 2 for 4..7, ... (at most 15 for 64 000 pixels).
fn log2_class(c: u32) -> usize {
    let mut v = c;
    let mut k = 0;
    while v > 1 {
        v = v >> 1;
        k += 1;
    }
    k
}

/// Palette slot for RGB555 colour `k`: its own slot, else the nearest slot colour (cached).
#[inline(always)]
unsafe fn slot_for(k: usize) -> u8 {
    let cached = LUT[k];
    if cached != UNKNOWN {
        return cached;
    }
    let own = SLOT_OF[k];
    let slot = if own != 0 {
        (own - 1) as u8
    } else {
        nearest_slot(k)
    };
    LUT[k] = slot;
    slot
}

/// The slot whose colour is closest to RGB555 colour `k` (0 if the palette is empty).
unsafe fn nearest_slot(k: usize) -> u8 {
    let (r, g, b) = ((k >> 10) as i32, ((k >> 5) & 31) as i32, (k & 31) as i32);
    let mut best = 0;
    let mut best_d = 0x7FFF_FFFF;
    let mut s = 0;
    while s < SLOTS {
        let c = PAL15[s];
        if c != FREE {
            let dr = (c >> 10) as i32 - r;
            let dg = ((c >> 5) & 31) as i32 - g;
            let db = (c & 31) as i32 - b;
            // weighted like the eye: green counts most, blue least
            let d = 3 * dr * dr + 4 * dg * dg + 2 * db * db;
            if d < best_d {
                best_d = d;
                best = s;
            }
        }
        s += 1;
    }
    best as u8
}

/// RGBA framebuffer of the renderer -> palette indices. Neighbouring pixels often have the
/// same colour (panel, backgrounds, doubled demo pixels): then the index is simply repeated.
unsafe fn convert_frame(fb: usize) {
    let back = (&raw mut BACK) as usize;
    let mut last = *(fb as *const u32) ^ 1; // differs from the first pixel
    let mut idx = 0u8;
    let mut i = 0;
    while i < 64000 {
        let c = *((fb + i * 4) as *const u32);
        if c != last {
            last = c;
            idx = slot_for(key15(c));
        }
        *((back + i) as *mut u8) = idx;
        i += 1;
    }
}

/// 6-bit DAC value of a 5-bit channel.
fn dac6(v: u32) -> u32 {
    (v << 1) | (v >> 4)
}

/// Writes the changed palette slots to the VGA DAC.
unsafe fn write_dac() {
    let mut s = 0;
    while s < 256 {
        if DIRTY[s] != 0 {
            let c = PAL15[s];
            outb(0x3C8, s as u32);
            outb(0x3C9, dac6(c >> 10));
            outb(0x3C9, dac6((c >> 5) & 31));
            outb(0x3C9, dac6(c & 31));
            DIRTY[s] = 0;
        }
        s += 1;
    }
}

unsafe fn wait_retrace() {
    let mut n = 0;
    while inb(0x3DA) & 8 != 0 && n < 1_000_000 {
        n += 1;
    }
    n = 0;
    while inb(0x3DA) & 8 == 0 && n < 1_000_000 {
        n += 1;
    }
}

unsafe fn show_frame() {
    let vram = phys(0xA0000);
    let mut i = 0;
    while i < 16000 {
        *((vram + i * 4) as *mut u32) = *(((&raw const BACK) as usize + i * 4) as *const u32);
        i += 1;
    }
}

// ------------------------------------------------------------------ self-test (Linux)

/// Plays `secs` seconds of a song without hardware (16-bit stereo 44.1 kHz into a buffer),
/// drawing 70 frames per second, and writes the last frame as palette indices (64000
/// bytes) followed by the palette (768 bytes, 0..255) to `out`. `view` 0..6 shows that view
/// throughout, 7 steps through all of them, 8 shows the spectrum in 3D, 9 runs the
/// benchmark with one second per scene (the last frame shows its table); plus 16 · n to
/// draw only n frames per second, as on a slow PC. Used by dos/selftest.asm.
#[no_mangle]
pub extern "C" fn rust_selftest(m: *const u8, len: u32, secs: u32, view: u32, out: *mut u8) -> u32 {
    // view (0..6, 7 = all in turn, 8 = spectrum 3D, 9 = benchmark with 1 s per scene)
    // + 16 * fps: draw `fps` frames per second (default 70), like a slow PC;
    // + 4096: 8-bit mono at 22 kHz like a Sound Blaster Pro
    // + 8192: only the views without command-line options
    let defaults = view >= 8192;
    let view = view % 8192;
    let pro = view >= 4096;
    let view = view % 4096;
    let fps = if view >= 16 { view / 16 } else { 70 };
    let view = view % 16;
    unsafe {
        BASE = 0;
        SB.sixteen = !pro;
        SB.rate = if pro {
            1_000_000 / (1_000_000 / 22050)
        } else {
            44100
        };
        static mut FAKE_DMA: [u8; 16384] = [0; 16384];
        if !setup_dma_buffer((&raw const FAKE_DMA) as u32, 16384) {
            return 1;
        }
        if mp_load_ptr(m, len as usize, SB.rate as f32) <= 0 {
            return 2;
        }
        vis_event(2);
        vis_set_playing(1);
        if defaults {
            vis_set_views(DEFAULT_VIEWS);
        }
        clear_lut();
        let mut views = 0;
        let target = if view == 8 { 2 } else { view }; // 8: the spectrum in 3D
        let mut presses = 0;
        while target < 7 && vis_current_view() != target && presses < 8 {
            vis_key(118);
            presses += 1;
        }
        if view == 8 {
            vis_key(51);
        }
        if view == 9 {
            vis_bench_start(1.0); // 9: benchmark with one second per scene
        }
        let frames = secs * fps;
        let mut f = 0;
        while f < frames {
            let now = f as f64 * 1000.0 / fps as f64;
            // the "interrupt": a block whenever its half has been played
            while (N_IRQ as f64) * BLOCK as f64 * 1000.0 / SB.rate as f64 <= now {
                rust_sb_irq_selftest();
            }
            if PEND_READY != 0 {
                commit_pending();
            }
            if view == 7 && f % (fps * 4) == fps * 4 - 1 && views < 6 {
                vis_key(118);
                views += 1;
            }
            let fb = vis_frame(now, SW, SH, 0.5) as usize;
            if f == 0 {
                vis_set_fine(0);
            }
            if f % PALETTE_EVERY == 0 {
                update_palette(fb);
            }
            convert_frame(fb);
            f += 1;
        }
        let mut i = 0;
        while i < 64000 {
            *((out as usize + i) as *mut u8) = BACK[i];
            i += 1;
        }
        i = 0;
        while i < 256 {
            let c = if PAL15[i] == FREE { 0 } else { PAL15[i] };
            let o = out as usize + 64000 + i * 3;
            *(o as *mut u8) = (dac6(c >> 10) << 2) as u8;
            *((o + 1) as *mut u8) = (dac6((c >> 5) & 31) << 2) as u8;
            *((o + 2) as *mut u8) = (dac6(c & 31) << 2) as u8;
            i += 1;
        }
        MIXED
    }
}

unsafe fn rust_sb_irq_selftest() {
    let h = NEXT_HALF;
    fill_half(h);
    NEXT_HALF = h ^ 1;
    N_IRQ += 1;
}
