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

#[lang = "pointee_sized"] pub trait PointeeSized {}
#[lang = "meta_sized"] pub trait MetaSized: PointeeSized {}
#[lang = "sized"] pub trait Sized: MetaSized {}
#[lang = "legacy_receiver"] pub trait LegacyReceiver {}
impl<T: PointeeSized> LegacyReceiver for &T {}
impl<T: PointeeSized> LegacyReceiver for &mut T {}
#[lang = "clone"] pub trait Clone: Sized { fn clone(&self) -> Self; }
#[lang = "copy"] pub trait Copy: Clone {}
#[lang = "freeze"] pub unsafe auto trait Freeze {}
#[lang = "sync"] pub unsafe auto trait Sync {}
#[lang = "unpin"] pub auto trait Unpin {}
#[lang = "unsafe_unpin"] pub unsafe auto trait UnsafeUnpin {}
#[lang = "drop_glue"] #[allow(unconditional_recursion)]
pub unsafe fn drop_glue<T: PointeeSized>(_p: *mut T) {}

macro_rules! copy { ($($t:ty)*) => { $(
    impl Clone for $t { fn clone(&self) -> Self { *self } }
    impl Copy for $t {}
)* } }
copy!(bool u8 i8 u16 u32 i32 u64 i64 usize isize f32 f64);
impl<T: PointeeSized> Clone for *const T { fn clone(&self) -> Self { *self } }
impl<T: PointeeSized> Copy for *const T {}
impl<T: PointeeSized> Clone for *mut T { fn clone(&self) -> Self { *self } }
impl<T: PointeeSized> Copy for *mut T {}
impl<T: PointeeSized> Clone for &T { fn clone(&self) -> Self { *self } }
impl<T: PointeeSized> Copy for &T {}

#[lang = "add"] pub trait Add<R = Self> { type Output; fn add(self, r: R) -> Self::Output; }
#[lang = "sub"] pub trait Sub<R = Self> { type Output; fn sub(self, r: R) -> Self::Output; }
#[lang = "mul"] pub trait Mul<R = Self> { type Output; fn mul(self, r: R) -> Self::Output; }
#[lang = "div"] pub trait Div<R = Self> { type Output; fn div(self, r: R) -> Self::Output; }
#[lang = "rem"] pub trait Rem<R = Self> { type Output; fn rem(self, r: R) -> Self::Output; }
#[lang = "bitand"] pub trait BitAnd<R = Self> { type Output; fn bitand(self, r: R) -> Self::Output; }
#[lang = "bitxor"] pub trait BitXor<R = Self> { type Output; fn bitxor(self, r: R) -> Self::Output; }
#[lang = "bitor"] pub trait BitOr<R = Self> { type Output; fn bitor(self, r: R) -> Self::Output; }
#[lang = "shl"] pub trait Shl<R = Self> { type Output; fn shl(self, r: R) -> Self::Output; }
#[lang = "shr"] pub trait Shr<R = Self> { type Output; fn shr(self, r: R) -> Self::Output; }
#[lang = "neg"] pub trait Neg { type Output; fn neg(self) -> Self::Output; }
#[lang = "not"] pub trait Not { type Output; fn not(self) -> Self::Output; }
#[lang = "add_assign"] pub trait AddAssign<R = Self> { fn add_assign(&mut self, r: R); }
#[lang = "sub_assign"] pub trait SubAssign<R = Self> { fn sub_assign(&mut self, r: R); }
#[lang = "mul_assign"] pub trait MulAssign<R = Self> { fn mul_assign(&mut self, r: R); }
#[lang = "div_assign"] pub trait DivAssign<R = Self> { fn div_assign(&mut self, r: R); }

macro_rules! bin { ($tr:ident $m:ident $op:tt; $($t:ty)*) => { $(
    impl $tr for $t { type Output = $t; fn $m(self, r: $t) -> $t { self $op r } }
)* } }
macro_rules! asg { ($tr:ident $m:ident $op:tt; $($t:ty)*) => { $(
    impl $tr for $t { fn $m(&mut self, r: $t) { *self $op r; } }
)* } }
bin!(Add add +; i32 u32 usize f32 f64 i64 u64 u8);
bin!(Sub sub -; i32 u32 usize f32 f64 i64 u64 u8);
bin!(Mul mul *; i32 u32 usize f32 f64 i64 u64);
bin!(Div div /; i32 u32 usize f32 f64 i64 u64);
bin!(Rem rem %; i32 u32 usize i64 u64);
bin!(BitAnd bitand &; u32 i32 usize u64 i64);
bin!(BitOr bitor |; u32 usize u64);
bin!(BitXor bitxor ^; u32 usize);
bin!(Shl shl <<; u32 i32 usize u64);
bin!(Shr shr >>; u32 i32 usize u64);
asg!(AddAssign add_assign +=; i32 u32 usize f32 f64 i64 u64);
asg!(SubAssign sub_assign -=; i32 u32 usize f32 f64 i64 u64);
asg!(MulAssign mul_assign *=; f32 f64);
asg!(DivAssign div_assign /=; u32 i32 usize f32 f64);
impl Neg for i32 { type Output = i32; fn neg(self) -> i32 { -self } }
impl Neg for i64 { type Output = i64; fn neg(self) -> i64 { -self } }
impl Neg for f32 { type Output = f32; fn neg(self) -> f32 { -self } }
impl Neg for f64 { type Output = f64; fn neg(self) -> f64 { -self } }
impl Not for bool { type Output = bool; fn not(self) -> bool { !self } }

#[lang = "eq"] pub trait PartialEq<R: PointeeSized = Self>: PointeeSized {
    fn eq(&self, o: &R) -> bool;
    fn ne(&self, o: &R) -> bool { !self.eq(o) }
}
#[lang = "partial_ord"] pub trait PartialOrd<R: PointeeSized = Self>: PartialEq<R> + PointeeSized {
    fn lt(&self, o: &R) -> bool;
    fn le(&self, o: &R) -> bool;
    fn gt(&self, o: &R) -> bool;
    fn ge(&self, o: &R) -> bool;
}
macro_rules! cmp { ($($t:ty)*) => { $(
    impl PartialEq for $t { fn eq(&self, o: &$t) -> bool { *self == *o } fn ne(&self, o: &$t) -> bool { *self != *o } }
    impl PartialOrd for $t {
        fn lt(&self, o: &$t) -> bool { *self < *o }
        fn le(&self, o: &$t) -> bool { *self <= *o }
        fn gt(&self, o: &$t) -> bool { *self > *o }
        fn ge(&self, o: &$t) -> bool { *self >= *o }
    }
)* } }
cmp!(bool u8 u32 i32 usize f32 f64 i64 u64);

#[lang = "index"] pub trait Index<I> { type Output: PointeeSized; fn index(&self, i: I) -> &Self::Output; }
#[lang = "index_mut"] pub trait IndexMut<I>: Index<I> { fn index_mut(&mut self, i: I) -> &mut Self::Output; }
impl<T, const N: usize> Index<usize> for [T; N] { type Output = T; fn index(&self, i: usize) -> &T { &self[i] } }
impl<T, const N: usize> IndexMut<usize> for [T; N] { fn index_mut(&mut self, i: usize) -> &mut T { &mut self[i] } }

#[lang = "panic_location"] pub struct Location<'a> { file: &'a str, line: u32, col: u32 }
#[lang = "panic_bounds_check"] fn panic_bounds_check(_i: usize, _l: usize) -> ! { loop {} }
#[lang = "panic_const_div_by_zero"] fn panic_div_zero() -> ! { loop {} }
#[lang = "panic_const_rem_by_zero"] fn panic_rem_zero() -> ! { loop {} }
#[lang = "panic_const_div_overflow"] fn panic_div_ovf() -> ! { loop {} }
#[lang = "panic_const_rem_overflow"] fn panic_rem_ovf() -> ! { loop {} }

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
#[path = "../src/vis_ui.rs"]
mod vis_ui;
#[path = "../src/vis_voxel.rs"]
mod vis_voxel;
#[path = "../src/api.rs"]
pub mod api;
