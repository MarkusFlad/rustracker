//! Dependency-free ProTracker MOD replayer for WebAssembly (and native).
//!
//! * `replayer` – parser, effects and mixer, written in a core-language-only
//!   subset (no iterators, Option, slices or allocation) so it also builds in
//!   a `#![no_core]` environment (preview/minicore.rs).
//! * `fft` – reusable radix-2 FFT with Hann window (same subset), `spectrum` – band,
//!   peak and log-axis helpers on top of its power spectrum.
//! * `math` – the maths the subset lacks (sin/cos, log, pow, sqrt, clamps).
//! * `gfx`, `font`, `vis` (+ `vis_osc`, `vis_mandel`, `vis_spec`, `vis_cube`, `vis_ui`) –
//!   the renderer of the framebuffer edition: every pixel of its user interface is
//!   drawn here (same subset).
//! * `api` – C-ABI exports for JavaScript (`mp_load`, `mp_render`, …).
//!
//! On wasm32 the crate is `no_std`; natively it links std so tests and the
//! `mod2wav` tool can use it.
#![cfg_attr(target_arch = "wasm32", no_std)]
#![warn(missing_docs)]
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
    clippy::manual_rem_euclid,
    clippy::manual_swap,
    clippy::zero_ptr,
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::excessive_precision,
    clippy::new_without_default
)]

pub mod api;
pub mod fft;
pub mod font;
pub mod gfx;
pub mod math;
pub mod replayer;
pub mod spectrum;
pub mod vis;
mod vis_cube;
mod vis_demo;
mod vis_lake;
mod vis_mandel;
mod vis_osc;
mod vis_spec;
mod vis_ui;
mod vis_voxel;
pub mod voxel_space;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
