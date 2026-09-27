//! The 8-bit math QMK's animations are written against, ported from lib8tion.
//!
//! QMK builds lib8tion with `FASTLED_SCALE8_FIXED=1`, so `scale8` and
//! `scale16by8` round up by one step: `i * (scale + 1) >> 8` rather than
//! `i * scale >> 8`. Porting an animation without that form leaves every
//! brightness one step low, which is why the fixed form is what lives here.
//!
//! The approximations are kept bit-for-bit, including their quirks:
//! [`atan2_8`] returns a wrapped `u8` for `dy < 0`, exactly as the C original
//! does, so animations that use it keep their shape.

/// Scale a byte by a fraction whose denominator is 256: `i * scale / 256`.
pub const fn scale8(i: u8, scale: u8) -> u8 {
    ((i as u16 * (1 + scale as u16)) >> 8) as u8
}

/// Scale a 16-bit value by a fraction whose denominator is 256.
pub const fn scale16by8(i: u16, scale: u8) -> u16 {
    ((i as u32 * (1 + scale as u32)) >> 8) as u16
}

/// Add two bytes, saturating at `u8::MAX`.
pub const fn qadd8(i: u8, j: u8) -> u8 {
    i.saturating_add(j)
}

/// Subtract two bytes, saturating at zero.
pub const fn qsub8(i: u8, j: u8) -> u8 {
    i.saturating_sub(j)
}

/// Absolute value of a signed byte, as lib8tion's `abs8(int8_t)`.
///
/// `abs8(-128)` is `-128` in C, where the negation happens in `int` and the
/// result is stored back into an `int8_t`; `wrapping_neg` keeps that.
pub const fn abs8(i: i8) -> i8 {
    if i < 0 { i.wrapping_neg() } else { i }
}

/// Interpolation table of lib8tion's 8-bit sine: `{b, m16}` pairs per quadrant.
const B_M16_INTERLEAVE: [u8; 8] = [0, 49, 49, 41, 90, 27, 117, 10];

/// Fast 8-bit sine, input angle 0-255, output 0-255 centred on 128.
pub fn sin8(theta: u8) -> u8 {
    let mut offset = theta;
    if theta & 0x40 != 0 {
        offset = 255u8.wrapping_sub(offset);
    }
    offset &= 0x3F;
    let mut secoffset = offset & 0x0F;
    if theta & 0x40 != 0 {
        secoffset += 1;
    }
    let section = (offset >> 4) as usize;
    let b = B_M16_INTERLEAVE[section * 2];
    let m16 = B_M16_INTERLEAVE[section * 2 + 1];
    let mx = m16.wrapping_mul(secoffset) >> 4;
    let mut y = mx.wrapping_add(b) as i8;
    if theta & 0x80 != 0 {
        y = y.wrapping_neg();
    }
    // The C version stores this sum back into an `int8_t` and only then returns
    // it as `uint8_t`, so the wrap is what produces 255 for the crest.
    (y as u8).wrapping_add(128)
}

/// Fast 8-bit cosine, input angle 0-255, output 0-255 centred on 128.
pub fn cos8(theta: u8) -> u8 {
    sin8(theta.wrapping_add(64))
}

/// Fast 8-bit `atan2`, returned as an angle over 0-255.
///
/// For `dy < 0` the C original computes a negative angle and converts it to
/// `uint8_t`, so the result wraps into 224-255 instead of continuing at 0. The
/// wrap is reproduced here: animations using this effect draw the same
/// discontinuity QMK draws.
pub fn atan2_8(dy: i16, dx: i16) -> u8 {
    if dy == 0 {
        return if dx >= 0 { 0 } else { 128 };
    }
    let abs_y = if dy > 0 { dy } else { -dy };
    let a = if dx >= 0 {
        32 - (32 * (dx - abs_y) / (dx + abs_y))
    } else {
        96 - (32 * (dx + abs_y) / (abs_y - dx))
    };
    let a = a as i8;
    if dy < 0 { (-(a as i32)) as u8 } else { a as u8 }
}

/// Integer square root of a 16-bit value, truncating.
pub fn sqrt16(x: u16) -> u8 {
    if x <= 1 {
        return x as u8;
    }
    let mut low: u8 = 1;
    let mut hi: u8 = if x > 7904 { 255 } else { (x >> 5) as u8 + 8 };
    loop {
        let mid = ((low as u16 + hi as u16) >> 1) as u8;
        if (mid as u16 * mid as u16) > x {
            hi = mid - 1;
        } else {
            if mid == 255 {
                return 255;
            }
            low = mid + 1;
        }
        if hi < low {
            return low - 1;
        }
    }
}

/// lib8tion's 16-bit linear congruential generator.
///
/// The seed starts at lib8tion's `RAND16_SEED`, so a given animation replays
/// identically on every boot, the way it does in QMK.
pub struct Rand16 {
    seed: u16,
}

impl Rand16 {
    /// lib8tion seeds the generator with `RAND16_SEED`, which is 1337.
    pub const fn new() -> Self {
        Self { seed: 1337 }
    }

    pub fn next_u16(&mut self) -> u16 {
        self.seed = self.seed.wrapping_mul(2053).wrapping_add(13849);
        self.seed
    }

    /// Random byte, mixed from both halves of the generator state.
    pub fn random8(&mut self) -> u8 {
        let seed = self.next_u16();
        (seed & 0xFF) as u8 + (seed >> 8) as u8
    }

    /// Random byte in `0..lim`.
    pub fn random8_max(&mut self, lim: u8) -> u8 {
        ((self.random8() as u16 * lim as u16) >> 8) as u8
    }

    /// Random byte in `min..lim`.
    pub fn random8_min_max(&mut self, min: u8, lim: u8) -> u8 {
        self.random8_max(lim.wrapping_sub(min)).wrapping_add(min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale8_uses_the_fixed_form() {
        // The fixed form reaches 255 at full scale; `i * scale >> 8` would stop at 254.
        assert_eq!(scale8(255, 255), 255);
        assert_eq!(scale8(255, 0), 0);
        assert_eq!(scale8(128, 127), 64);
        assert_eq!(scale16by8(65535, 255), 65535);
        assert_eq!(scale16by8(1000, 0), 3);
    }

    #[test]
    fn sin8_spans_the_full_range() {
        assert_eq!(sin8(0), 128);
        assert_eq!(sin8(64), 255);
        assert_eq!(sin8(128), 128);
        assert_eq!(sin8(192), 1);
        assert_eq!(cos8(0), 255);
        assert_eq!(cos8(64), 128);
    }

    #[test]
    fn atan2_8_matches_the_c_original_including_the_wrap() {
        assert_eq!(atan2_8(0, 1), 0);
        assert_eq!(atan2_8(0, -1), 128);
        assert_eq!(atan2_8(1, 1), 32);
        // The C `return -a` converts a negative angle to uint8_t, so the
        // lower half of the circle comes back as 224-255 rather than 0-32.
        assert_eq!(atan2_8(-1, 1), 224);
    }

    #[test]
    fn sqrt16_truncates() {
        assert_eq!(sqrt16(0), 0);
        assert_eq!(sqrt16(1), 1);
        assert_eq!(sqrt16(2), 1);
        assert_eq!(sqrt16(9), 3);
        assert_eq!(sqrt16(65535), 255);
    }

    #[test]
    fn random8_is_deterministic_from_the_lib8tion_seed() {
        assert_eq!(Rand16::new().random8(), Rand16::new().random8());
        assert!(Rand16::new().random8_max(3) < 3);
        assert_eq!(Rand16::new().next_u16(), 1337u16.wrapping_mul(2053).wrapping_add(13849));
    }
}
