//! Fast Fourier Transform, reusable for any power-of-two size up to [`FFT_MAX`].
//!
//! Iterative radix-2 Cooley-Tukey FFT (decimation in time). Typical use:
//!
//! 1. [`Fft::setup`] once per size (twiddle factors and Hann window),
//! 2. load the input: [`Fft::load_samples`] (linear buffer), [`Fft::load_ring`]
//!    (the newest samples of a ring buffer) or [`Fft::set_input`] (complex, unwindowed),
//! 3. [`Fft::transform`], then read [`Fft::output`] or call [`Fft::compute_power`]
//!    to fill [`Fft::power`] (scaled so a full-scale sine gives about 1.0).
//!
//! [`Fft::analyse_ring`] and [`Fft::analyse`] do steps 2 and 3 in one call.
//! Like the replayer it only uses the core language, so it also builds with
//! `preview/minicore.rs`; sine and cosine come from [`crate::math`].

use crate::math::{cos, sin, PI};

/// Largest supported transform size.
pub const FFT_MAX: usize = 4096;

/// FFT state: work buffers, twiddle factors, window and the last power spectrum.
pub struct Fft {
    n: usize,
    re: [f32; FFT_MAX],
    im: [f32; FFT_MAX],
    window: [f32; FFT_MAX],
    tw_cos: [f32; FFT_MAX],
    tw_sin: [f32; FFT_MAX],
    /// Power spectrum (amplitude²) of the last [`Fft::compute_power`], bins `0 ..= n/2`.
    pub power: [f32; FFT_MAX],
}

impl Fft {
    /// An FFT without a size; call [`Fft::setup`] before use. `const`, so it can
    /// initialise a static.
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

    /// The configured transform size (0 before [`Fft::setup`]).
    pub fn size(&self) -> usize {
        self.n
    }

    /// Prepares twiddle factors and the Hann window for size `n` (a power of two
    /// from 16 to [`FFT_MAX`]). Returns false for an unsupported size; calling it
    /// again with the current size costs nothing.
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

    /// Loads `n` real samples from a linear buffer, applying the Hann window.
    ///
    /// # Safety
    /// `samples` must point to at least `size()` readable f32 values.
    pub unsafe fn load_samples(&mut self, samples: *const f32) {
        let mut i = 0;
        while i < self.n {
            let v = *((samples as usize + i * 4) as *const f32);
            self.re[i] = v * self.window[i];
            self.im[i] = 0.0;
            i += 1;
        }
    }

    /// Loads the newest `n` samples of a ring buffer (`ring_len` entries, the newest
    /// sample just before index `ring_pos`), applying the Hann window. Does nothing
    /// if the ring is shorter than the transform.
    ///
    /// # Safety
    /// `ring` must point to `ring_len` readable f32 values.
    pub unsafe fn load_ring(&mut self, ring: *const f32, ring_len: usize, ring_pos: usize) {
        let n = self.n;
        if n == 0 || ring_len < n {
            return;
        }
        let mut i = 0;
        while i < n {
            let idx = (ring_pos + ring_len - n + i) % ring_len;
            let v = *((ring as usize + idx * 4) as *const f32);
            self.re[i] = v * self.window[i];
            self.im[i] = 0.0;
            i += 1;
        }
    }

    /// Sets one complex input value directly (no window).
    pub fn set_input(&mut self, i: usize, re: f32, im: f32) {
        self.re[i] = re;
        self.im[i] = im;
    }

    /// One complex output bin of the last [`Fft::transform`].
    pub fn output(&self, i: usize) -> (f32, f32) {
        (self.re[i], self.im[i])
    }

    /// In-place forward FFT of the loaded input.
    pub fn transform(&mut self) {
        self.bit_reverse();
        let mut len = 2;
        while len <= self.n {
            self.butterflies(len);
            len = len << 1;
        }
    }

    /// Reorders the input into bit-reversed index order.
    fn bit_reverse(&mut self) {
        let n = self.n;
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
                self.swap(i, j);
            }
            i += 1;
        }
    }

    fn swap(&mut self, i: usize, j: usize) {
        let tr = self.re[i];
        self.re[i] = self.re[j];
        self.re[j] = tr;
        let ti = self.im[i];
        self.im[i] = self.im[j];
        self.im[j] = ti;
    }

    /// All butterflies of one stage with sub-transform length `len`.
    fn butterflies(&mut self, len: usize) {
        let half = len >> 1;
        let tstep = self.n / len;
        let mut start = 0;
        while start < self.n {
            let mut k = 0;
            while k < half {
                self.butterfly(start + k, start + k + half, k * tstep);
                k += 1;
            }
            start += len;
        }
    }

    #[inline(always)]
    fn butterfly(&mut self, a: usize, b: usize, tw: usize) {
        let wr = self.tw_cos[tw];
        let wi = 0.0 - self.tw_sin[tw];
        let xr = self.re[b] * wr - self.im[b] * wi;
        let xi = self.re[b] * wi + self.im[b] * wr;
        self.re[b] = self.re[a] - xr;
        self.im[b] = self.im[a] - xi;
        self.re[a] += xr;
        self.im[a] += xi;
    }

    /// Fills [`Fft::power`] (bins `0 ..= n/2`) from the last transform of a
    /// Hann-windowed real signal: amplitude², so a full-scale sine is about 1.0.
    pub fn compute_power(&mut self) {
        let n = self.n;
        // |X|·2/Σw with Σw = n/2 for the Hann window -> amplitude; we store amplitude²
        let scale = 4.0 / n as f32;
        let mut k = 0;
        while k <= n / 2 {
            let a = self.re[k] * scale;
            let b = self.im[k] * scale;
            self.power[k] = a * a + b * b;
            k += 1;
        }
    }

    /// Power spectrum of the newest `n` samples of a ring buffer
    /// ([`Fft::load_ring`] + [`Fft::transform`] + [`Fft::compute_power`]).
    ///
    /// # Safety
    /// `ring` must point to `ring_len` readable f32 values.
    pub unsafe fn analyse_ring(&mut self, ring: *const f32, ring_len: usize, ring_pos: usize) {
        if self.n == 0 || ring_len < self.n {
            return;
        }
        self.load_ring(ring, ring_len, ring_pos);
        self.transform();
        self.compute_power();
    }

    /// Power spectrum of `n` samples from a linear buffer.
    ///
    /// # Safety
    /// `samples` must point to at least `size()` readable f32 values.
    pub unsafe fn analyse(&mut self, samples: *const f32) {
        self.load_samples(samples);
        self.transform();
        self.compute_power();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_naive_dft() {
        let n = 256;
        let mut f = Box::new(Fft::empty());
        assert!(f.setup(n));
        let input: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                (
                    ((i * 37) % 17) as f32 / 8.0 - 1.0,
                    ((i * 11) % 7) as f32 / 4.0 - 0.7,
                )
            })
            .collect();
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
            assert!(
                (fr as f64 - sr).abs() < 1e-3 && (fi as f64 - si).abs() < 1e-3,
                "bin {k}: {fr},{fi} vs {sr},{si}"
            );
        }
    }

    fn peak_bin(f: &Fft, n: usize) -> (usize, f32) {
        f.power[..=n / 2]
            .iter()
            .copied()
            .enumerate()
            .fold(
                (0, 0f32),
                |(bk, bv), (k, v)| if v > bv { (k, v) } else { (bk, bv) },
            )
    }

    #[test]
    fn sine_lands_in_its_bin() {
        let n = 4096;
        let sr = 44100.0f64;
        let freq = 1000.0f64;
        let ring: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr).sin() as f32 * 0.5)
            .collect();
        let mut f = Box::new(Fft::empty());
        f.setup(n);
        unsafe { f.analyse_ring(ring.as_ptr(), n, 0) };
        let (best, bp) = peak_bin(&f, n);
        let bin_hz = sr / n as f64;
        assert!(
            ((best as f64 * bin_hz) - freq).abs() < bin_hz,
            "peak at bin {best}"
        );
        // amplitude 0.5 -> power ~0.25 (Hann scalloping can lower it a bit)
        assert!(bp > 0.12 && bp < 0.3, "power {bp}");
        // the linear-buffer path gives the same result
        let ring_power: Vec<f32> = f.power[..=n / 2].to_vec();
        unsafe { f.analyse(ring.as_ptr()) };
        assert_eq!(&f.power[..=n / 2], &ring_power[..]);
    }
}
