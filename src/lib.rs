//! Dependency-free ProTracker MOD replayer for WebAssembly (and native).
//!
//! * `replayer` – parser, effects and mixer, written in a core-language-only
//!   subset (no iterators, Option, slices or allocation) so it also builds in
//!   a `#![no_core]` environment (preview/minicore.rs).
//! * `fft` – radix-2 FFT with Hann window for the spectrum view (same subset).
//! * `api` – C-ABI exports for JavaScript (`mp_load`, `mp_render`, …).
//!
//! On wasm32 the crate is `no_std`; natively it links std so tests and the
//! `mod2wav` tool can use it.
#![cfg_attr(target_arch = "wasm32", no_std)]

pub mod api;
pub mod fft;
pub mod replayer;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
