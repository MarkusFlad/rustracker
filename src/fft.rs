// Fast Fourier Transform for the spectrum view.
//
// Iterative radix-2 Cooley-Tukey FFT (decimation in time) with a Hann window.
// Like replayer.rs it only uses the core language (no `core` methods, no std
// math), so the sine for the twiddle factors and the window is computed here
// with a Taylor polynomial. Sizes: powers of two from 16 to FFT_MAX.

pub const FFT_MAX: usize = 4096;
const PI: f64 = 3.141592653589793;

/// sin(x) for any x, accurate to about 1e-15 (range reduction + Taylor series to x^17).
pub fn sin(x: f64) -> f64 {
    let mut x = x;
    let two_pi = 2.0 * PI;
    if x > PI || x < 0.0 - PI {
        // x - 2π·round(x / 2π) without a floor function
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
    let x2 = x * x;
    x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0
        * (1.0 - x2 / 110.0 * (1.0 - x2 / 156.0 * (1.0 - x2 / 210.0 * (1.0 - x2 / 272.0))))))))
}

pub fn cos(x: f64) -> f64 {
    sin(x + PI / 2.0)
}

pub struct Fft {
    n: usize,
    re: [f32; FFT_MAX],
    im: [f32; FFT_MAX],
    window: [f32; FFT_MAX],
    tw_cos: [f32; FFT_MAX],
    tw_sin: [f32; FFT_MAX],
    /// Power spectrum of the last transform, bins 0 .. n/2.
    pub power: [f32; FFT_MAX],
}

impl Fft {
    pub const fn empty() -> Fft {
        Fft {
            n: 0,
            re: [0.0; FFT_MAX],
            im: [0.0; FFT_MAX],
            window: [0.0; FFT_MAX],
            tw_cos: [0.0; FFT_MAX],
            tw_sin: [0.0; FFT_MAX],
            power: [0.0; FFT_MAX],
        }
    }

    pub fn size(&self) -> usize {
        self.n
    }

    /// Prepare twiddle factors and the Hann window for size `n` (power of two).
    /// Returns false for an unsupported size.
    pub fn setup(&mut self, n: usize) -> bool {
        if n < 16 || n > FFT_MAX || (n & (n - 1)) != 0 {
            return false;
        }
        if n == self.n {
            return true;
        }
        self.n = n;
        let mut k = 0;
        while k < n / 2 {
            let a = 2.0 * PI * k as f64 / n as f64;
            self.tw_cos[k] = cos(a) as f32;
            self.tw_sin[k] = sin(a) as f32;
            k += 1;
        }
        let mut i = 0;
        while i < n {
            self.window[i] = (0.5 - 0.5 * cos(2.0 * PI * i as f64 / n as f64)) as f32;
            i += 1;
        }
        true
    }

    /// Load `n` samples from a ring buffer (`ring_len` entries, the newest sample
    /// just before `ring_pos`), window them, transform, and fill `power`.
    /// power[k] is scaled so that a full-scale sine gives about 1.0 in its bin.
    pub fn analyse_ring(&mut self, ring: *const f32, ring_len: usize, ring_pos: usize) {
        let n = self.n;
        if n == 0 || ring_len < n {
            return;
        }
        let mut i = 0;
        while i < n {
            let idx = (ring_pos + ring_len - n + i) % ring_len;
            let v = unsafe { *((ring as usize + idx * 4) as *const f32) };
            self.re[i] = v * self.window[i];
            self.im[i] = 0.0;
            i += 1;
        }
        self.transform();
        // |X|·2/Σw with Σw = n/2 for the Hann window -> amplitude; we return amplitude²
        let scale = 4.0 / n as f32;
        let mut k = 0;
        while k <= n / 2 {
            let a = self.re[k] * scale;
            let b = self.im[k] * scale;
            self.power[k] = a * a + b * b;
            k += 1;
        }
    }

    /// In-place complex FFT of re/im (size n).
    pub fn transform(&mut self) {
        let n = self.n;
        // bit-reversal permutation
        let mut j = 0;
        let mut i = 1;
        while i < n {
            let mut bit = n >> 1;
            while (j & bit) != 0 {
                j = j ^ bit;
                bit = bit >> 1;
            }
            j = j ^ bit;
            if i < j {
                let tr = self.re[i];
                self.re[i] = self.re[j];
                self.re[j] = tr;
                let ti = self.im[i];
                self.im[i] = self.im[j];
                self.im[j] = ti;
            }
            i += 1;
        }
        // butterflies
        let mut len = 2;
        while len <= n {
            let half = len >> 1;
            let tstep = n / len;
            let mut start = 0;
            while start < n {
                let mut k = 0;
                while k < half {
                    let wr = self.tw_cos[k * tstep];
                    let wi = 0.0 - self.tw_sin[k * tstep];
                    let a = start + k;
                    let b = a + half;
                    let xr = self.re[b] * wr - self.im[b] * wi;
                    let xi = self.re[b] * wi + self.im[b] * wr;
                    self.re[b] = self.re[a] - xr;
                    self.im[b] = self.im[a] - xi;
                    self.re[a] += xr;
                    self.im[a] += xi;
                    k += 1;
                }
                start += len;
            }
            len = len << 1;
        }
    }

    pub fn set_input(&mut self, i: usize, re: f32, im: f32) {
        self.re[i] = re;
        self.im[i] = im;
    }
    pub fn output(&self, i: usize) -> (f32, f32) {
        (self.re[i], self.im[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_accurate() {
        let mut x = -20.0;
        while x < 20.0 {
            assert!((sin(x) - x.sin()).abs() < 1e-12, "sin({x})");
            assert!((cos(x) - x.cos()).abs() < 1e-12, "cos({x})");
            x += 0.0137;
        }
    }

    #[test]
    fn matches_naive_dft() {
        let n = 256;
        let mut f = Box::new(Fft::empty());
        assert!(f.setup(n));
        let input: Vec<(f32, f32)> = (0..n).map(|i| (((i * 37) % 17) as f32 / 8.0 - 1.0, ((i * 11) % 7) as f32 / 4.0 - 0.7)).collect();
        for (i, &(r, im)) in input.iter().enumerate() {
            f.set_input(i, r, im);
        }
        f.transform();
        for k in 0..n {
            let (mut sr, mut si) = (0f64, 0f64);
            for (t, &(r, im)) in input.iter().enumerate() {
                let a = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                sr += r as f64 * a.cos() - im as f64 * a.sin();
                si += r as f64 * a.sin() + im as f64 * a.cos();
            }
            let (fr, fi) = f.output(k);
            assert!((fr as f64 - sr).abs() < 1e-3 && (fi as f64 - si).abs() < 1e-3, "bin {k}: {fr},{fi} vs {sr},{si}");
        }
    }

    #[test]
    fn sine_lands_in_its_bin() {
        let n = 4096;
        let sr = 44100.0f64;
        let freq = 1000.0f64;
        let ring: Vec<f32> = (0..n).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr).sin() as f32 * 0.5).collect();
        let mut f = Box::new(Fft::empty());
        f.setup(n);
        f.analyse_ring(ring.as_ptr(), n, 0);
        let (mut best, mut bp) = (0, 0f32);
        for k in 0..=n / 2 {
            if f.power[k] > bp {
                bp = f.power[k];
                best = k;
            }
        }
        let bin_hz = sr / n as f64;
        assert!(((best as f64 * bin_hz) - freq).abs() < bin_hz, "peak at bin {best}");
        // amplitude 0.5 -> power ~0.25 (Hann scalloping can lower it a bit)
        assert!(bp > 0.12 && bp < 0.3, "power {bp}");
    }
}
