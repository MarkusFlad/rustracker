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
// replayer.rs, fft.rs and api.rs deliberately avoid methods from `core` (clamp, contains,
// swap, is_multiple_of, consts::PI, ...) so they also build with preview/minicore.rs.
// These lints would suggest exactly those methods:
#![allow(
    clippy::approx_constant,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::deref_addrof,
    clippy::manual_clamp,
    clippy::manual_is_multiple_of,
    clippy::manual_range_contains,
    clippy::manual_swap,
    clippy::zero_ptr
)]

pub mod api;
pub mod fft;
pub mod replayer;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
