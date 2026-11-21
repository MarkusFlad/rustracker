//! DOS layer: runs the Rustracker on a PC with a Sound Blaster and VGA, in 32-bit protected
//! mode set up by RTRACK.COM (dos/stub.asm) and dos/entry.asm.
//!
//! * Sound: the song is mixed in the Sound Blaster's interrupt into the half of an
//!   auto-initialising DMA buffer that has just been played (SB16: 16-bit stereo 44.1 kHz,
//!   SB 2.0/Pro: 8-bit mono 22 kHz). Each block of 1024 frames also becomes a renderer slot
//!   (scopes, spectrum), exactly like the worklet's messages in the browser.
//! * Picture: the framebuffer renderer draws 320 x 200 RGBA frames; they are mapped onto a
//!   fixed 256-colour palette (6 x 6 x 6 colour cube plus 40 greys, ordered dithering) and
//!   copied into VGA memory during the vertical retrace (mode 13h).
//! * Keyboard: polled from the keyboard controller (its interrupt stays masked).
//!
//! Memory is addressed through segments with the image's load address as base: physical
//! address `p` is pointer `p - base` (the 4 GB limit makes this wrap around correctly).

use crate::api::*;

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
static mut SPEC_TURN: u32 = 0;
static mut SPEC_PTR: usize = 0;

// picture
static mut LUT: [u8; 32768] = [0; 32768];
static mut BACK: [u8; 64000] = [0; 64000];
const BAYER: [i32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

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
        log(b"RUSTRACKER fuer DOS\r\n");
        log(b"Programm bei ");
        log_hex(base, 8);
        log(b", Song bei ");
        log_hex(bi.mod_phys, 8);
        log(b" (");
        log_dec(bi.mod_len);
        log(b" Bytes), DMA-Speicher bei ");
        log_hex(bi.dma_phys, 8);
        log(b"\r\n");
        if bi.env_phys != 0 {
            parse_blaster(phys(bi.env_phys));
        }
        if bi.opt_phys != 0 {
            let o = phys(bi.opt_phys);
            let c = *(o as *const u8) as u32;
            if c == 84 || c == 116 {
                QUIT_MS = parse_num(o + 1, 10) as f64 * 1000.0;
            }
        }
        if !sb_reset() {
            log(b"Keine Sound-Blaster-Karte an Port ");
            log_hex(SB.port, 3);
            log(b" gefunden (BLASTER-Variable pruefen).\r\n");
            return 2;
        }
        let ver = sb_version();
        if ver < 0x200 {
            log(b"Sound Blaster mit DSP 2.0 oder neuer noetig.\r\n");
            return 2;
        }
        SB.sixteen = ver >= 0x400;
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
            log(b"Die MOD-Datei konnte nicht gelesen werden.\r\n");
            return 3;
        }
        if !setup_dma_buffer(bi.dma_phys, bi.dma_len) {
            log(b"Kein passender DMA-Puffer.\r\n");
            return 4;
        }
        log(b"Sound Blaster ");
        if SB.sixteen {
            log(b"16");
        } else {
            log(b"Pro");
        }
        log(b" an ");
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
        log(b" Hz, Puffer bei ");
        log_hex(SB.buf, 8);
        log(b"\r\n");
        vis_event(2);
        vis_set_playing(1);
        set_palette();
        build_tables();
        log(b"Start der Wiedergabe\r\n");
        sb_start();
        main_loop();
        sb_stop();
        log(b"Ende nach ");
        log_dec(peek32(&raw const N_IRQ));
        log(b" Bloecken (Sound-Blaster-Interrupts).\r\nTschuess!\r\n");
        0
    }
}

/// A hardware interrupt `irq` (0..15); dos/entry.asm saves the FPU state around it. The Sound
/// Blaster's interrupt mixes the next block; every interrupt is acknowledged at the PIC.
#[no_mangle]
pub extern "C" fn rust_irq(irq: u32) {
    unsafe {
        if irq == SB.irq {
            inb(SB.port + if SB.sixteen { 0x0F } else { 0x0E }); // acknowledge at the DSP
            let h = NEXT_HALF;
            fill_half(h);
            NEXT_HALF = h ^ 1;
            N_IRQ += 1;
        }
        if irq >= 8 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);
    }
}

// ------------------------------------------------------------------ main loop

/// Keyboard, renderer slots and frames until Escape.
unsafe fn main_loop() {
    let mut e0 = false;
    let mut first = true;
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
                return;
            }
        }
        if peek32(&raw const PEND_READY) != 0 {
            commit_pending();
        }
        let now = audio_ms();
        if QUIT_MS > 0.0 && now >= QUIT_MS {
            return;
        }
        let fb = vis_frame(now, SW, SH, 0.5) as usize;
        if first {
            vis_set_fine(0); // coarse voxels (the renderer sets its defaults on the first frame)
            first = false;
        }
        convert_frame(fb);
        wait_retrace();
        show_frame();
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
    // the 4096-point FFT costs a lot on a Pentium: a new spectrum every other slot
    SPEC_TURN = SPEC_TURN ^ 1;
    if SPEC_TURN != 0 || SPEC_PTR == 0 {
        SPEC_PTR = mp_spectrum(4096) as usize;
    }
    let sp = SPEC_PTR;
    let off = vis_spec_offset() as usize;
    i = 0;
    while i < 2049 {
        *((slot + (off + i) * 4) as *mut f32) = *((sp + i * 4) as *const f32);
        i += 1;
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

/// Palette: 6 x 6 x 6 colour cube (0..215) and 40 greys (216..255).
unsafe fn set_palette() {
    outb(0x3C8, 0);
    let mut i = 0;
    while i < 256 {
        let (r, g, b) = palette_rgb(i);
        outb(0x3C9, r >> 2);
        outb(0x3C9, g >> 2);
        outb(0x3C9, b >> 2);
        i += 1;
    }
}

/// RGB (0..255) of palette entry `i`.
fn palette_rgb(i: u32) -> (u32, u32, u32) {
    if i < 216 {
        ((i / 36) * 51, ((i / 6) % 6) * 51, (i % 6) * 51)
    } else {
        let v = (i - 216) * 255 / 39;
        (v, v, v)
    }
}

/// RGB555 -> nearest palette entry (cube or grey).
unsafe fn build_lut() {
    let mut c = 0;
    while c < 32768 {
        let r = ((c >> 10) & 31) * 255 / 31;
        let g = ((c >> 5) & 31) * 255 / 31;
        let b = (c & 31) * 255 / 31;
        let cube = ((r + 25) / 51) * 36 + ((g + 25) / 51) * 6 + (b + 25) / 51;
        let grey = 216 + ((r + g + b) / 3 * 39 + 127) / 255;
        LUT[c as usize] = if dist(r, g, b, cube) <= dist(r, g, b, grey) {
            cube as u8
        } else {
            grey as u8
        };
        c += 1;
    }
}

fn dist(r: u32, g: u32, b: u32, i: u32) -> u32 {
    let (pr, pg, pb) = palette_rgb(i);
    let dr = r as i32 - pr as i32;
    let dg = g as i32 - pg as i32;
    let db = b as i32 - pb as i32;
    (dr * dr + dg * dg + db * db) as u32
}

/// RGBA framebuffer of the renderer -> palette indices with 4 x 4 ordered dithering.
unsafe fn convert_frame(fb: usize) {
    let mut p = fb;
    let mut q = (&raw mut BACK) as usize;
    let mut y = 0;
    while y < SH as usize {
        let row = (y & 3) * 4;
        let d0 = DITHER[row] as usize;
        let d1 = DITHER[row + 1] as usize;
        let d2 = DITHER[row + 2] as usize;
        let d3 = DITHER[row + 3] as usize;
        let end = q + SW as usize;
        while q < end {
            convert_pixel(p, q, d0);
            convert_pixel(p + 4, q + 1, d1);
            convert_pixel(p + 8, q + 2, d2);
            convert_pixel(p + 12, q + 3, d3);
            p += 16;
            q += 4;
        }
        y += 1;
    }
}

/// One RGBA pixel at `p` -> palette index at `q`, with dither offset `d`.
#[inline(always)]
unsafe fn convert_pixel(p: usize, q: usize, d: usize) {
    // raw table reads: the indices are in range by construction, and bounds checks would
    // double the work of this innermost loop
    let c = *(p as *const u32) as usize;
    let cl = (&raw const CLAMP5) as usize + d;
    let r = *((cl + (c & 255)) as *const u8) as usize;
    let g = *((cl + ((c >> 8) & 255)) as *const u8) as usize;
    let b = *((cl + ((c >> 16) & 255)) as *const u8) as usize;
    *(q as *mut u8) = *(((&raw const LUT) as usize + ((r << 10) | (g << 5) | b)) as *const u8);
}

/// Dither offsets (+ 25, so they index [`CLAMP5`] without going negative).
static mut DITHER: [u8; 16] = [0; 16];
/// `CLAMP5[v + 25]` = `clamp(v, 0, 255) >> 3` for v in -25 ..= 280.
static mut CLAMP5: [u8; 306] = [0; 306];

/// Fills [`DITHER`], [`CLAMP5`] and the RGB555 table [`LUT`].
unsafe fn build_tables() {
    let mut i = 0;
    while i < 16 {
        DITHER[i] = (BAYER[i] * 51 / 16) as u8;
        i += 1;
    }
    i = 0;
    while i < 306 {
        let v = i as i32 - 25;
        CLAMP5[i] = ((if v < 0 {
            0
        } else if v > 255 {
            255
        } else {
            v
        }) >> 3) as u8;
        i += 1;
    }
    build_lut();
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
/// throughout, 7 steps through all of them. Used by dos/selftest.asm.
#[no_mangle]
pub extern "C" fn rust_selftest(m: *const u8, len: u32, secs: u32, view: u32, out: *mut u8) -> u32 {
    unsafe {
        BASE = 0;
        SB.sixteen = true;
        SB.rate = 44100;
        static mut FAKE_DMA: [u8; 16384] = [0; 16384];
        if !setup_dma_buffer((&raw const FAKE_DMA) as u32, 16384) {
            return 1;
        }
        if mp_load_ptr(m, len as usize, SB.rate as f32) <= 0 {
            return 2;
        }
        vis_event(2);
        vis_set_playing(1);
        build_tables();
        let mut views = 0;
        while views < view && view < 7 {
            vis_key(118);
            views += 1;
        }
        let frames = secs * 70;
        let mut f = 0;
        while f < frames {
            let now = f as f64 * 1000.0 / 70.0;
            // the "interrupt": a block whenever its half has been played
            while (N_IRQ as f64) * BLOCK as f64 * 1000.0 / SB.rate as f64 <= now {
                rust_sb_irq_selftest();
            }
            if PEND_READY != 0 {
                commit_pending();
            }
            if view >= 7 && f % (70 * 4) == 70 * 4 - 1 && views < 6 {
                vis_key(118);
                views += 1;
            }
            let fb = vis_frame(now, SW, SH, 0.5) as usize;
            if f == 0 {
                vis_set_fine(0);
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
            let (r, g, b) = palette_rgb(i as u32);
            *((out as usize + 64000 + i * 3) as *mut u8) = r as u8;
            *((out as usize + 64000 + i * 3 + 1) as *mut u8) = g as u8;
            *((out as usize + 64000 + i * 3 + 2) as *mut u8) = b as u8;
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
