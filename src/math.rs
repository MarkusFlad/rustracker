//! Small math library shared by the replayer, the FFT and the framebuffer renderer.
//!
//! Written in the core-language-only subset (see `replayer.rs`): no `core` methods and no
//! std math, so everything also builds with `preview/minicore.rs`. Float bits are reached
//! through a union; the transcendental functions are range reduction plus polynomials,
//! square roots use Newton steps.

/// π as f64.
pub const PI: f64 = 3.141592653589793;
const LN2: f64 = 0.6931471805599453;

union Bits {
    f: f64,
    u: u64,
}

fn to_bits(x: f64) -> u64 {
    unsafe { Bits { f: x }.u }
}
fn from_bits(u: u64) -> f64 {
    unsafe { Bits { u }.f }
}

// ------------------------------------------------------------------ integers

/// Smaller of two `i32`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is smaller.
pub fn imin(a: i32, b: i32) -> i32 {
    if a < b {
        a
    } else {
        b
    }
}
/// Larger of two `i32`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is larger.
pub fn imax(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}
/// Limits an `i32` to a range.
///
/// # Arguments
/// * `v` - value
/// * `lo` - lower bound
/// * `hi` - upper bound (`lo <= hi`)
///
/// # Returns
/// `v` limited to `lo ..= hi`.
pub fn iclamp(v: i32, lo: i32, hi: i32) -> i32 {
    imax(lo, imin(v, hi))
}
/// Absolute value of an `i32` (no overflow check for `i32::MIN`).
///
/// # Arguments
/// * `v` - value
///
/// # Returns
/// `|v|`.
pub fn iabs(v: i32) -> i32 {
    if v < 0 {
        0 - v
    } else {
        v
    }
}
/// Smaller of two `usize`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is smaller.
pub fn umin(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}
/// Smaller of two `u32`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is smaller.
pub fn u32min(a: u32, b: u32) -> u32 {
    if a < b {
        a
    } else {
        b
    }
}

// ------------------------------------------------------------------ floats

/// Absolute value of an `f64`.
///
/// # Arguments
/// * `x` - value
///
/// # Returns
/// `|x|`.
pub fn absf(x: f64) -> f64 {
    if x < 0.0 {
        0.0 - x
    } else {
        x
    }
}
/// Smaller of two `f64`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is smaller (`b` if they compare equal).
pub fn minf(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}
/// Larger of two `f64`.
///
/// # Arguments
/// * `a` - first value
/// * `b` - second value
///
/// # Returns
/// `a` or `b`, whichever is larger (`b` if they compare equal).
pub fn maxf(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}
/// Limits an `f64` to a range.
///
/// # Arguments
/// * `x` - value
/// * `lo` - lower bound
/// * `hi` - upper bound (`lo <= hi`)
///
/// # Returns
/// `x` limited to `lo ..= hi`.
pub fn clampf(x: f64, lo: f64, hi: f64) -> f64 {
    maxf(lo, minf(x, hi))
}
/// Limits a sample value to full scale (for oscilloscope traces).
///
/// # Arguments
/// * `v` - sample value
///
/// # Returns
/// `v` limited to `-1 ..= 1`.
pub fn clamp1(v: f32) -> f32 {
    if v > 1.0 {
        1.0
    } else if v < -1.0 {
        -1.0
    } else {
        v
    }
}
/// Rounds down.
///
/// # Arguments
/// * `x` - value, `|x| < 2^62`
///
/// # Returns
/// The largest integer `<= x`, as `f64`.
pub fn floor(x: f64) -> f64 {
    let i = x as i64;
    let f = i as f64;
    if f > x {
        f - 1.0
    } else {
        f
    }
}
/// Fractional part.
///
/// # Arguments
/// * `x` - value, `|x| < 2^62`
///
/// # Returns
/// `x - floor(x)`, in `0 .. 1`.
pub fn fract(x: f64) -> f64 {
    x - floor(x)
}

/// Sine, accurate to about 1e-15 (range reduction + Taylor series to x^17).
///
/// # Arguments
/// * `x` - angle in radians, any value
///
/// # Returns
/// sin(x) in `-1 ..= 1`.
pub fn sin(x: f64) -> f64 {
    let x = reduce_half_pi(x);
    let x2 = x * x;
    // Horner form of x - x^3/3! + x^5/5! - ... + x^17/17!
    let mut p = 1.0;
    let mut k = 8.0;
    while k >= 1.0 {
        p = 1.0 - x2 / ((2.0 * k) * (2.0 * k + 1.0)) * p;
        k -= 1.0;
    }
    x * p
}

/// Maps x to the range `-π/2 ..= π/2` with the same sine.
fn reduce_half_pi(x: f64) -> f64 {
    let mut x = x;
    let two_pi = 2.0 * PI;
    if x > PI || x < 0.0 - PI {
        // x - 2π·trunc(x / 2π) without a floor function
        let k = (x / two_pi) as i64;
        x = x - k as f64 * two_pi;
    }
    while x > PI {
        x -= two_pi;
    }
    while x < 0.0 - PI {
        x += two_pi;
    }
    if x > PI / 2.0 {
        x = PI - x;
    } else if x < 0.0 - PI / 2.0 {
        x = 0.0 - PI - x;
    }
    x
}

/// Cosine, accurate to about 1e-15.
///
/// # Arguments
/// * `x` - angle in radians, any value
///
/// # Returns
/// cos(x) in `-1 ..= 1`.
pub fn cos(x: f64) -> f64 {
    sin(x + PI / 2.0)
}

/// Binary logarithm, about 1e-12 relative accuracy.
///
/// # Arguments
/// * `x` - value, should be > 0
///
/// # Returns
/// log2(x), or -1100 for `x <= 0`.
pub fn log2(x: f64) -> f64 {
    if x <= 0.0 {
        return -1100.0;
    }
    let mut b = to_bits(x);
    let mut e = ((b >> 52) & 0x7ff) as i64;
    if e == 0 {
        // subnormal: scale up first
        b = to_bits(x * 18014398509481984.0); // 2^54
        e = ((b >> 52) & 0x7ff) as i64 - 54;
    }
    let mut m = from_bits((b & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000); // [1, 2)
    let mut ex = (e - 1023) as f64;
    if m > 1.4142135623730951 {
        m *= 0.5;
        ex += 1.0;
    }
    // ln(m) = 2·atanh(t) = 2·(t + t^3/3 + t^5/5 + ... + t^15/15), t = (m-1)/(m+1), |t| < 0.172
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let mut s = 2.0 / 15.0;
    let mut k = 13.0;
    while k >= 1.0 {
        s = 2.0 / k + t2 * s;
        k -= 2.0;
    }
    ex + t * s / LN2
}

/// Decimal logarithm.
///
/// # Arguments
/// * `x` - value, should be > 0
///
/// # Returns
/// log10(x) (very negative for `x <= 0`, see [`log2`]).
pub fn log10(x: f64) -> f64 {
    log2(x) * 0.30102999566398120
}

/// Power of two, about 1e-13 relative accuracy.
///
/// # Arguments
/// * `x` - exponent, `|x| < 1000`
///
/// # Returns
/// 2^x.
pub fn exp2(x: f64) -> f64 {
    if x < -1020.0 {
        return 0.0;
    }
    if x > 1020.0 {
        return from_bits(0x7fe0_0000_0000_0000);
    }
    let i = floor(x);
    let f = (x - i) * LN2; // 2^(x-i) = e^f, f in [0, ln2)
                           // Taylor series of e^f up to f^15 (error < 1e-15 for f < ln 2)
    let mut p = 1.0;
    let mut k = 15.0;
    while k >= 1.0 {
        p = 1.0 + f / k * p;
        k -= 1.0;
    }
    let e = (i as i64 + 1023) as u64;
    p * from_bits(e << 52)
}

/// Power for positive bases.
///
/// # Arguments
/// * `x` - base, should be > 0
/// * `y` - exponent
///
/// # Returns
/// x^y, or 0 for `x <= 0`.
pub fn pow(x: f64, y: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    exp2(y * log2(x))
}

/// Square root (estimate via exp2/log2, refined by two Newton steps).
///
/// # Arguments
/// * `x` - value
///
/// # Returns
/// √x, or 0 for `x <= 0`.
pub fn sqrt(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = exp2(0.5 * log2(x));
    y = 0.5 * (y + x / y);
    0.5 * (y + x / y)
}

/// Decay factor for exponential smoothing.
///
/// # Arguments
/// * `k` - rate × time, `k >= 0`
///
/// # Returns
/// e^(-k), in `0 ..= 1`.
pub fn exp_neg(k: f64) -> f64 {
    exp2(0.0 - k / LN2)
}

/// Decibels of a power value (amplitude², 1.0 = full scale).
///
/// # Arguments
/// * `p` - power, `>= 0`
///
/// # Returns
/// 10·log10(p), with a floor of -140 dB.
pub fn power_db(p: f64) -> f64 {
    10.0 * log10(p + 1e-14)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_are_accurate() {
        let mut x = 1e-6;
        while x < 1e6 {
            assert!((log2(x) - x.log2()).abs() < 1e-10, "log2({x})");
            assert!(
                (sqrt(x) - x.sqrt()).abs() < 1e-10 * x.sqrt().max(1.0),
                "sqrt({x})"
            );
            x *= 1.37;
        }
        let mut y = -40.0;
        while y < 40.0 {
            assert!(
                (exp2(y) - y.exp2()).abs() < 1e-12 * y.exp2().max(1.0),
                "exp2({y})"
            );
            assert_eq!(floor(y), y.floor());
            assert!((sin(y) - y.sin()).abs() < 1e-12, "sin({y})");
            assert!((cos(y) - y.cos()).abs() < 1e-12, "cos({y})");
            y += 0.173;
        }
        assert!((pow(3.0, 2.5) - 3f64.powf(2.5)).abs() < 1e-10);
        assert!((exp_neg(2.0) - (-2f64).exp()).abs() < 1e-12);
        assert_eq!(imin(3, -2), -2);
        assert_eq!(iclamp(70, 0, 64), 64);
        assert_eq!(iabs(-5), 5);
    }
}
