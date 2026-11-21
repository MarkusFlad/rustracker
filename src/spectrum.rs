//! Reusable helpers for power spectra, e.g. [`crate::fft::Fft::power`].
//!
//! [`Spectrum`] is a read-only view of a power array with its bin width, and answers
//! the questions the views ask: the power within a frequency band (log-frequency bars,
//! spectrogram rows) and the strongest peak with sub-bin precision. [`LogScale`] maps
//! frequencies onto a logarithmic axis. Core-language subset like the rest of the crate.

use crate::math::{clampf, floor, log2, pow, power_db};

/// A power spectrum: `len` values (bins `0 .. len`) at `p`, `bin_hz` Hz per bin.
pub struct Spectrum {
    p: *const f32,
    len: usize,
    bin_hz: f64,
}

/// Result of [`Spectrum::peak`].
pub struct Peak {
    /// False if no bin in the range had any noticeable power.
    pub found: bool,
    /// Peak frequency in Hz (parabolic interpolation between bins).
    pub freq: f64,
    /// Power of the peak bin in dB.
    pub db: f64,
}

impl Spectrum {
    /// A read-only view of a power spectrum.
    ///
    /// # Arguments
    /// * `p` - first power value (bin 0)
    /// * `len` - number of bins
    /// * `bin_hz` - width of one bin in Hz (sample rate / FFT size)
    ///
    /// # Returns
    /// The view.
    ///
    /// # Safety
    /// `p` must point to `len` readable f32 values that outlive the view.
    pub unsafe fn new(p: *const f32, len: usize, bin_hz: f64) -> Spectrum {
        Spectrum { p, len, bin_hz }
    }

    /// Power of one bin.
    ///
    /// # Arguments
    /// * `i` - bin index; bins past the end read as the last bin
    ///
    /// # Returns
    /// The power of bin `i`.
    pub fn bin(&self, i: usize) -> f64 {
        let i = if i < self.len { i } else { self.len - 1 };
        unsafe { *((self.p as usize + i * 4) as *const f32) as f64 }
    }

    /// Strongest power within a frequency band. A band narrower than one bin interpolates linearly
    /// between the two nearest bins.
    ///
    /// # Arguments
    /// * `f0` - lower edge in Hz
    /// * `f1` - upper edge in Hz (`f1 > f0`)
    ///
    /// # Returns
    /// The power (not dB) of the band.
    pub fn band(&self, f0: f64, f1: f64) -> f64 {
        let k0 = f0 / self.bin_hz;
        let k1 = f1 / self.bin_hz;
        if k1 - k0 < 1.0 {
            let kk = (k0 + k1) / 2.0;
            let i = kk as usize;
            let t = kk - i as f64;
            return self.bin(i) * (1.0 - t) + self.bin(i + 1) * t;
        }
        let mut v = 0.0;
        let mut i = k0 as usize;
        let mut end = k1 as usize + 1;
        if end > self.len - 1 {
            end = self.len - 1;
        }
        while i <= end {
            let x = self.bin(i);
            if x > v {
                v = x;
            }
            i += 1;
        }
        v
    }

    /// The strongest bin within a frequency range, refined by fitting a parabola through the dB
    /// values of it and its neighbours.
    ///
    /// # Arguments
    /// * `fmin` - lower limit in Hz
    /// * `fmax` - upper limit in Hz
    ///
    /// # Returns
    /// A [`Peak`]; `found` is false if no bin had noticeable power (< 1e-7).
    pub fn peak(&self, fmin: f64, fmax: f64) -> Peak {
        let none = Peak {
            found: false,
            freq: 0.0,
            db: 0.0,
        };
        let kb = self.strongest_bin(fmin, fmax);
        if kb == 0 || self.bin(kb) < 1e-7 {
            return none;
        }
        let a = power_db(self.bin(kb - 1));
        let b = power_db(self.bin(kb));
        let c = power_db(self.bin(kb + 1));
        let den = a - 2.0 * b + c;
        let off = if den != 0.0 { 0.5 * (a - c) / den } else { 0.0 };
        Peak {
            found: true,
            freq: (kb as f64 + off) * self.bin_hz,
            db: b,
        }
    }

    /// Index of the strongest bin strictly inside the range (0 if none).
    fn strongest_bin(&self, fmin: f64, fmax: f64) -> usize {
        let mut kb = 0;
        let mut i = (fmin / self.bin_hz) as usize + 1;
        let end = (fmax / self.bin_hz) as usize;
        while i < end && i + 1 < self.len {
            if kb == 0 || self.bin(i) > self.bin(kb) {
                kb = i;
            }
            i += 1;
        }
        kb
    }
}

/// Maps a power value onto a normalised dB scale.
///
/// # Arguments
/// * `power` - power value (amplitude²)
/// * `db_min` - dB value that maps to 0, e.g. -96
///
/// # Returns
/// `0 ..= 1`: 0 at `db_min` and below, 1 at 0 dB and above.
pub fn unit_db(power: f64, db_min: f64) -> f64 {
    clampf((power_db(power) - db_min) / (0.0 - db_min), 0.0, 1.0)
}

/// The MIDI note nearest to a frequency (A4 = 440 Hz = 69).
///
/// # Arguments
/// * `freq` - frequency in Hz, > 0
///
/// # Returns
/// The MIDI note number.
pub fn midi_note(freq: f64) -> i32 {
    floor(69.0 + 12.0 * log2(freq / 440.0) + 0.5) as i32
}

/// A logarithmic frequency axis from `fmin` to `fmax`.
pub struct LogScale {
    /// Lowest frequency (left edge / band 0).
    pub fmin: f64,
    /// Highest frequency (right edge).
    pub fmax: f64,
}

impl LogScale {
    /// A logarithmic frequency axis.
    ///
    /// # Arguments
    /// * `fmin` - frequency at the left edge in Hz, > 0
    /// * `fmax` - frequency at the right edge in Hz, > `fmin`
    ///
    /// # Returns
    /// The axis.
    pub fn new(fmin: f64, fmax: f64) -> LogScale {
        LogScale { fmin, fmax }
    }
    /// Frequency ratio between neighbouring band edges when the axis is split into equal bands;
    /// band `i` spans `fmin·r^i .. fmin·r^(i+1)`.
    ///
    /// # Arguments
    /// * `n` - number of bands
    ///
    /// # Returns
    /// The ratio `r = (fmax/fmin)^(1/n)`.
    pub fn ratio(&self, n: usize) -> f64 {
        pow(self.fmax / self.fmin, 1.0 / n as f64)
    }
    /// Position of a frequency on the axis.
    ///
    /// # Arguments
    /// * `f` - frequency in Hz
    /// * `w` - axis width (pixels or any unit)
    ///
    /// # Returns
    /// 0 at `fmin`, `w` at `fmax`, logarithmic in between.
    pub fn x(&self, f: f64, w: f64) -> f64 {
        log2(f / self.fmin) / log2(self.fmax / self.fmin) * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_and_peaks() {
        // 2049 bins of 10 Hz, a peak at bin 100 (1000 Hz) with a slightly louder right neighbour
        let mut p = vec![1e-9f32; 2049];
        p[99] = 0.01;
        p[100] = 0.5;
        p[101] = 0.02;
        let s = unsafe { Spectrum::new(p.as_ptr(), p.len(), 10.0) };
        assert_eq!(s.band(950.0, 1050.0), 0.5f32 as f64);
        let pk = s.peak(40.0, 5000.0);
        assert!(
            pk.found && pk.freq > 1000.0 && pk.freq < 1005.0,
            "{}",
            pk.freq
        );
        assert_eq!(midi_note(440.0), 69);
        assert!((unit_db(1.0, -96.0) - 1.0).abs() < 1e-9);
        let ls = LogScale::new(30.0, 20000.0);
        assert!((ls.x(20000.0, 100.0) - 100.0).abs() < 1e-9);
    }
}
