//! Colour types and the HSV→RGB conversion the animations are written against.

/// Hue, saturation and value, each 0-255.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hsv {
    pub h: u8,
    pub s: u8,
    pub v: u8,
}

impl Hsv {
    pub const BLACK: Hsv = Hsv::new(0, 0, 0);

    pub const fn new(h: u8, s: u8, v: u8) -> Self {
        Self { h, s, v }
    }
}

/// Red, green and blue, each 0-255.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Convert HSV to RGB, as QMK's `hsv_to_rgb`.
///
/// The CIE1931 gamma curve is not applied: QMK only uses it when a keyboard
/// defines `USE_CIE1931_CURVE`, which nothing in QMK's own sources does.
pub fn hsv_to_rgb(hsv: Hsv) -> Rgb {
    if hsv.s == 0 {
        return Rgb::new(hsv.v, hsv.v, hsv.v);
    }

    let h = hsv.h as u16;
    let s = hsv.s as u16;
    let v = hsv.v as u16;

    let region = h * 6 / 255;
    let remainder = ((h * 2 - region * 85) * 3) as u8;

    let p = ((v * (255 - s)) >> 8) as u8;
    let q = ((v * (255 - ((s * remainder as u16) >> 8))) >> 8) as u8;
    let t = ((v * (255 - ((s * (255 - remainder as u16)) >> 8))) >> 8) as u8;

    match region {
        0 | 6 => Rgb::new(v as u8, t, p),
        1 => Rgb::new(q, v as u8, p),
        2 => Rgb::new(p, v as u8, t),
        3 => Rgb::new(p, q, v as u8),
        4 => Rgb::new(t, p, v as u8),
        _ => Rgb::new(v as u8, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_hues_land_on_the_primary_colours() {
        assert_eq!(hsv_to_rgb(Hsv::new(0, 0, 255)), Rgb::new(255, 255, 255));
        assert_eq!(hsv_to_rgb(Hsv::new(0, 255, 255)), Rgb::new(255, 0, 0));
        assert_eq!(hsv_to_rgb(Hsv::new(85, 255, 255)), Rgb::new(0, 255, 0));
        assert_eq!(hsv_to_rgb(Hsv::new(170, 255, 255)), Rgb::new(0, 0, 255));
        assert_eq!(hsv_to_rgb(Hsv::new(0, 255, 0)), Rgb::BLACK);
    }
}
