// Only for build environments without an installed wasm32 target:
// a minimal stand-in for `core`, just enough for the replayer, the FFT, the framebuffer renderer and the API.
//
//   RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown \
//     --crate-type cdylib -C opt-level=3 -C panic=abort -C overflow-checks=no \
//     -o modplayer.wasm preview/minicore.rs
//
// With a normal toolchain use `cargo build --release --target wasm32-unknown-unknown`.
#![feature(no_core, lang_items, rustc_attrs, auto_traits)]
#![no_core]
#![allow(internal_features, dead_code, unused)]

#[path = "minicore_lang.rs"]
mod minicore_lang;
pub use minicore_lang::*;

#[path = "../src/replayer.rs"]
pub mod replayer;
#[path = "../src/fft.rs"]
pub mod fft;
#[path = "../src/math.rs"]
pub mod math;
#[path = "../src/spectrum.rs"]
pub mod spectrum;
#[path = "../src/font.rs"]
pub mod font;
#[path = "../src/gfx.rs"]
pub mod gfx;
#[path = "../src/vis.rs"]
pub mod vis;
#[path = "../src/voxel_space.rs"]
pub mod voxel_space;
#[path = "../src/vis_demo.rs"]
mod vis_demo;
#[path = "../src/vis_cube.rs"]
mod vis_cube;
#[path = "../src/vis_lake.rs"]
mod vis_lake;
#[path = "../src/vis_mandel.rs"]
mod vis_mandel;
#[path = "../src/vis_osc.rs"]
mod vis_osc;
#[path = "../src/vis_spec.rs"]
mod vis_spec;
#[path = "../src/vis_bench.rs"]
mod vis_bench;
#[path = "../src/vis_ui.rs"]
mod vis_ui;
#[path = "../src/vis_voxel.rs"]
mod vis_voxel;
#[path = "../src/api.rs"]
pub mod api;
