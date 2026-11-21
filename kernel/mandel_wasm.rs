// Real-time Mandelbrot kernel for WebAssembly (SIMD128) - from our Mandelbrot zoomer.
//
// Same ideas as the fast benchmark version:
//  - SIMD: f64x2 vectors (wasm SIMD128 has 128-bit registers),
//  - several independent dependency chains per call: GROUP = 4 vectors
//    = 8 pixels are iterated interleaved, which hides the mul/add latency,
//  - parallelism: one instance per Web Worker, each renders a band of rows.
// Unlike the benchmark, the zoomer needs the escape iteration of every pixel
// (for smooth coloring), so each lane counts its iterations with a mask and is
// frozen once it has escaped.
//
// New for the MOD tracker: with `pal == 0` the kernel writes palette *indices*
// (0xffffffff = inside) instead of colours. The framebuffer edition needs them to
// shift the palette in time with the music and to blend while it reprojects.
//
// The file is #![no_core]: the few language items it needs are declared in `mini`.
// Build (also done by the CI):
//   RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
//       -C opt-level=3 -C target-feature=+simd128 -C panic=abort -C strip=symbols \
//       -o www-fb/mandel.wasm kernel/mandel_wasm.rs

#![feature(no_core, lang_items, repr_simd, intrinsics, rustc_attrs, auto_traits)]
#![no_core]
#![no_std]
#![allow(internal_features, non_camel_case_types, dead_code, unnecessary_transmutes)]

mod mini {
    #[lang = "pointee_sized"] pub trait PointeeSized {}
    #[lang = "meta_sized"] pub trait MetaSized: PointeeSized {}
    #[lang = "sized"] pub trait Sized: MetaSized {}
    #[lang = "copy"] pub trait Copy {}
    #[lang = "freeze"] pub unsafe auto trait Freeze {}
    #[lang = "legacy_receiver"] pub trait LegacyReceiver {}
    impl<T: ?Sized> LegacyReceiver for &T {}
    impl<T: ?Sized> LegacyReceiver for &mut T {}
    impl<T: ?Sized> Copy for *mut T {}
    impl<T: ?Sized> Copy for *const T {}
    impl<T: ?Sized> Copy for &T {}
    impl<T: Copy, const N: usize> Copy for [T; N] {}

    macro_rules! copy { ($($t:ty)*) => { $(impl Copy for $t {})* } }
    copy!(f64 f32 u8 u32 i32 u64 i64 usize bool);

    macro_rules! binop {
        ($lang:literal, $tr:ident, $f:ident, $op:tt, $($t:ty)*) => {
            #[lang = $lang] pub trait $tr<R = Self> { type Output; fn $f(self, r: R) -> Self::Output; }
            $(impl $tr for $t { type Output = $t; #[inline(always)] fn $f(self, r: $t) -> $t { self $op r } })*
        };
    }
    binop!("add", Add, add, +, f64 u32 i32 u64 i64 usize);
    binop!("sub", Sub, sub, -, f64 u32 i32 u64 i64 usize);
    binop!("mul", Mul, mul, *, f64 u32 i32 u64 i64 usize);
    binop!("div", Div, div, /, f64);
    binop!("bitand", BitAnd, bitand, &, u32 u64 i64 usize);
    binop!("bitor", BitOr, bitor, |, u32 u64 i64);

    #[lang = "shr"] pub trait Shr<R = Self> { type Output; fn shr(self, r: R) -> Self::Output; }
    impl Shr<u32> for u64 { type Output = u64; #[inline(always)] fn shr(self, r: u32) -> u64 { self >> r } }
    #[lang = "neg"] pub trait Neg { type Output; fn neg(self) -> Self::Output; }
    impl Neg for f64 { type Output = f64; #[inline(always)] fn neg(self) -> f64 { -self } }

    #[lang = "eq"] pub trait PartialEq<R: ?Sized = Self> { fn eq(&self, o: &R) -> bool; fn ne(&self, o: &R) -> bool; }
    #[lang = "partial_ord"] pub trait PartialOrd<R: ?Sized = Self>: PartialEq<R> {
        fn lt(&self, o: &R) -> bool; fn le(&self, o: &R) -> bool;
        fn gt(&self, o: &R) -> bool; fn ge(&self, o: &R) -> bool;
    }
    macro_rules! cmp { ($($t:ty)*) => { $(
        impl PartialEq for $t {
            #[inline(always)] fn eq(&self, o: &$t) -> bool { *self == *o }
            #[inline(always)] fn ne(&self, o: &$t) -> bool { *self != *o }
        }
        impl PartialOrd for $t {
            #[inline(always)] fn lt(&self, o: &$t) -> bool { *self < *o }
            #[inline(always)] fn le(&self, o: &$t) -> bool { *self <= *o }
            #[inline(always)] fn gt(&self, o: &$t) -> bool { *self > *o }
            #[inline(always)] fn ge(&self, o: &$t) -> bool { *self >= *o }
        }
    )* } }
    cmp!(f64 u32 i32 u64 i64 usize);

    #[repr(simd)] pub struct f64x2(pub [f64; 2]);
    #[repr(simd)] pub struct i64x2(pub [i64; 2]);
    impl Copy for f64x2 {}
    impl Copy for i64x2 {}

    #[rustc_intrinsic] pub unsafe fn simd_add<T>(a: T, b: T) -> T;
    #[rustc_intrinsic] pub unsafe fn simd_sub<T>(a: T, b: T) -> T;
    #[rustc_intrinsic] pub unsafe fn simd_mul<T>(a: T, b: T) -> T;
    #[rustc_intrinsic] pub unsafe fn simd_le<T, U>(a: T, b: T) -> U;
    #[rustc_intrinsic] pub unsafe fn simd_or<T>(a: T, b: T) -> T;
    #[rustc_intrinsic] pub unsafe fn simd_select<M, T>(m: M, a: T, b: T) -> T;
    #[rustc_intrinsic] pub unsafe fn simd_reduce_or<T, U>(x: T) -> U;
    #[rustc_intrinsic] pub unsafe fn transmute<T, U>(x: T) -> U;
}

use mini::*;

const GROUP: usize = 4; // 4 x f64x2 = 8 pixels, independent chains

#[inline(always)]
fn splat(v: f64) -> f64x2 { f64x2([v, v]) }

/// log2 for x >= 1 (good to ~1e-4, enough for smooth coloring).
#[inline(always)]
fn log2(x: f64) -> f64 {
    let bits: u64 = unsafe { transmute(x) };
    let e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let m: f64 = unsafe { transmute((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000u64) };
    // m in [1, 2): log2(m) via t = (m-1)/(m+1), log2(m) = 2/ln2 * (t + t^3/3 + t^5/5 + t^7/7)
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let p = t * (2.0 + t2 * (2.0 / 3.0 + t2 * (2.0 / 5.0 + t2 * (2.0 / 7.0 + t2 * (2.0 / 9.0)))));
    e as f64 + p * 1.4426950408889634
}

/// Renders `rows` rows of `stride` pixels (stride multiple of 8) as RGBA u32.
/// Pixel (x, y) is c = (re0 + x*step, im0 - y*step).
/// Palette: `pal_len` (power of two) RGBA entries at `pal`; pal = 0 writes palette
/// indices instead (0xffffffff for points inside the set).
#[no_mangle]
pub unsafe extern "C" fn render(out: usize, stride: u32, rows: u32,
                                re0: f64, im0: f64, step: f64, max_iter: u32,
                                pal: usize, pal_len: u32, color_density: f64, color_offset: f64) {
    let four = splat(4.0);
    let two = splat(2.0);
    let zero_i = i64x2([0, 0]);
    let mut y = 0u32;
    while y < rows {
        let ci = splat(im0 - y as f64 * step);
        let mut x = 0u32;
        while x < stride {
            // c for 8 pixels
            let mut cr = [splat(0.0); GROUP];
            let mut k = 0usize;
            while k < GROUP {
                let px = x as f64 + (2 * k) as f64;
                cr = set(cr, k, f64x2([re0 + px * step, re0 + (px + 1.0) * step]));
                k = k + 1;
            }
            let [c0, c1, c2, c3] = cr;
            let (mut zr0, mut zr1, mut zr2, mut zr3) = (c0, c1, c2, c3);
            let (mut zi0, mut zi1, mut zi2, mut zi3) = (ci, ci, ci, ci);
            let (mut n0, mut n1, mut n2, mut n3) = (zero_i, zero_i, zero_i, zero_i);
            let (mut m0, mut m1, mut m2, mut m3) = (zero_i, zero_i, zero_i, zero_i);

            let mut it = 0u32;
            while it < max_iter {
                // 4 iterations between the "all escaped?" checks
                let mut j = 0u32;
                while j < 4 {
                    macro_rules! step {
                        ($zr:ident, $zi:ident, $c:ident, $n:ident, $m:ident) => {
                            let r2 = simd_mul($zr, $zr);
                            let i2 = simd_mul($zi, $zi);
                            $m = simd_le::<f64x2, i64x2>(simd_add(r2, i2), four);   // still inside?
                            $n = simd_sub($n, $m);                                // count + 1 (mask = -1)
                            let nzi = simd_add(simd_mul(simd_mul(two, $zr), $zi), ci);
                            let nzr = simd_add(simd_sub(r2, i2), $c);
                            $zr = simd_select($m, nzr, $zr);                      // freeze escaped lanes
                            $zi = simd_select($m, nzi, $zi);
                        };
                    }
                    step!(zr0, zi0, c0, n0, m0);
                    step!(zr1, zi1, c1, n1, m1);
                    step!(zr2, zi2, c2, n2, m2);
                    step!(zr3, zi3, c3, n3, m3);
                    j = j + 1;
                }
                it = it + 4;
                let any: i64 = simd_reduce_or(simd_or(simd_or(m0, m1), simd_or(m2, m3)));
                if any == 0 { break; }
            }

            let base = out + ((y * stride + x) as usize) * 4;
            let ctx = Shade { max_iter: max_iter as i64, pal, mask: (pal_len - 1) as u64,
                              density: color_density, offset: color_offset };
            write2(base, &ctx, zr0, zi0, n0);
            write2(base + 8, &ctx, zr1, zi1, n1);
            write2(base + 16, &ctx, zr2, zi2, n2);
            write2(base + 24, &ctx, zr3, zi3, n3);
            x = x + 8;
        }
        y = y + 1;
    }
}

struct Shade { max_iter: i64, pal: usize, mask: u64, density: f64, offset: f64 }

#[inline(always)]
fn set(a: [f64x2; GROUP], k: usize, v: f64x2) -> [f64x2; GROUP] {
    let [a0, a1, a2, a3] = a;
    if k == 0 { [v, a1, a2, a3] } else if k == 1 { [a0, v, a2, a3] }
    else if k == 2 { [a0, a1, v, a3] } else { [a0, a1, a2, v] }
}

#[inline(always)]
unsafe fn write2(addr: usize, s: &Shade, zr: f64x2, zi: f64x2, n: i64x2) {
    let [r0, r1]: [f64; 2] = transmute(zr);
    let [i0, i1]: [f64; 2] = transmute(zi);
    let [n0, n1]: [i64; 2] = transmute(n);
    *(addr as *mut u32) = shade(s, r0 * r0 + i0 * i0, n0);
    *((addr + 4) as *mut u32) = shade(s, r1 * r1 + i1 * i1, n1);
}

/// Smooth coloring: nu = n + 1 - log2(log2 |z|), mapped into the palette
/// (or returned as an index when there is no palette).
#[inline(always)]
unsafe fn shade(s: &Shade, mag2: f64, n: i64) -> u32 {
    if n >= s.max_iter {
        // inside: black (ABGR in memory = RGBA bytes), or the "inside" index
        return if s.pal == 0 { 0xffffffff } else { 0xff000000 };
    }
    let mut nu = n as f64 + 1.0 - log2(log2(mag2) * 0.5);
    if nu < 0.0 { nu = 0.0; }
    let idx = ((nu * s.density + s.offset) as u64) & s.mask;
    if s.pal == 0 {
        return idx as u32;
    }
    *((s.pal + (idx as usize) * 4) as *const u32)
}
