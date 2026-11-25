// Crate root of the DOS build: the whole library (replayer, FFT, renderer, API) plus the DOS
// layer (dos/dos.rs: Sound Blaster, VGA mode 13h, keyboard), compiled for 32-bit x86 without
// `core` (the minicore stand-in from preview/) and linked by dos/build.sh with the protected
// mode entry (dos/entry.asm) into RTRACK.BIN, which RTRACK.COM (dos/stub.asm) loads and runs.
#![feature(no_core, lang_items, rustc_attrs, auto_traits)]
#![no_core]
#![allow(internal_features, dead_code, unused)]

#[path = "../preview/minicore_lang.rs"]
mod minicore_lang;
pub use minicore_lang::*;

#[path = "../src/api.rs"]
pub mod api;
#[path = "../src/fft.rs"]
pub mod fft;
#[path = "../src/font.rs"]
pub mod font;
#[path = "../src/gfx.rs"]
pub mod gfx;
#[path = "../src/math.rs"]
pub mod math;
#[path = "../src/replayer.rs"]
pub mod replayer;
#[path = "../src/spectrum.rs"]
pub mod spectrum;
#[path = "../src/vis.rs"]
pub mod vis;
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
#[path = "../src/voxel_space.rs"]
pub mod voxel_space;

mod dos;
