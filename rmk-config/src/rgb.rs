//! Per-key RGB configuration, read from `rgb.toml`.
//!
//! `rgb.toml` sits next to `keyboard.toml` and is layered on top of it as one
//! more config source, so `[rgb_matrix]` and `[ws2812]` arrive in the same
//! [`KeyboardTomlConfig`](crate::KeyboardTomlConfig). Key names and defaults
//! follow QMK's `info.json`, which is where these options are documented:
//! <https://docs.qmk.fm/features/rgb_matrix>.
//!
//! This module checks structure only: counts, ranges and matrix positions. The
//! animation names are validated by `rmk-macro`, which is the crate that knows
//! which effects RMK implements.

use std::collections::BTreeMap;

use serde::Deserialize;

/// File name of the per-key RGB config, next to `keyboard.toml`.
pub const RGB_TOML_FILE: &str = "rgb.toml";

/// LED driver. `ws2812` is the only one RMK drives itself today.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RgbDriver {
    Ws2812,
}

/// Byte order the chain expects, spelling `WS2812_BYTE_ORDER` in QMK.
///
/// The `*w` variants send a fourth white channel, for SK6812RGBW-style parts.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorOrder {
    Rgb,
    Grb,
    Bgr,
    Rgbw,
    Grbw,
}

impl ColorOrder {
    /// Colour channels per LED on the wire.
    pub fn channels(self) -> u8 {
        match self {
            ColorOrder::Rgb | ColorOrder::Grb | ColorOrder::Bgr => 3,
            ColorOrder::Rgbw | ColorOrder::Grbw => 4,
        }
    }
}

/// WS2812 chain wiring and bit timings.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ws2812Config {
    /// Data pin, written like the pins in `keyboard.toml` (`GPIO4`, `PIN_6`, ...).
    pub pin: String,
    #[serde(default = "default_color_order")]
    pub color_order: ColorOrder,
    /// One bit period in nanoseconds (`WS2812_TIMING`).
    #[serde(default = "default_timing_ns")]
    pub timing_ns: u16,
    /// High phase of a `1` bit (`WS2812_T1H`).
    #[serde(default = "default_t1h_ns")]
    pub t1h_ns: u16,
    /// High phase of a `0` bit (`WS2812_T0H`).
    #[serde(default = "default_t0h_ns")]
    pub t0h_ns: u16,
    /// Latch pulse after the last bit (`WS2812_TRST_US`).
    #[serde(default = "default_reset_us")]
    pub reset_us: u16,
}

/// One LED of the chain, in chain order.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedConfig {
    /// Electrical matrix position `[row, col]`. Omit for a LED that has no key,
    /// such as underglow; such a LED is unreachable from key events.
    pub matrix: Option<[u8; 2]>,
    /// Horizontal position, in QMK's `0..=224` coordinate space.
    pub x: u8,
    /// Vertical position, in QMK's `0..=64` coordinate space.
    pub y: u8,
    /// LED flags (`LED_FLAG_*` in QMK). Defaults to no flags.
    #[serde(default)]
    pub flags: u8,
}

/// State the chain starts in, when nothing is stored yet.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RgbDefaultConfig {
    #[serde(default = "default_on")]
    pub on: bool,
    /// Effect name, as used in the Vial effect list. Must be enabled under
    /// `[rgb_matrix.animations]`.
    #[serde(default = "default_animation")]
    pub animation: String,
    #[serde(default)]
    pub hue: u8,
    #[serde(default = "default_sat")]
    pub sat: u8,
    /// Brightness. Defaults to `max_brightness`, as in QMK.
    #[serde(default)]
    pub val: Option<u8>,
    #[serde(default = "default_speed")]
    pub speed: u8,
    #[serde(default = "default_flags")]
    pub flags: u8,
}

impl Default for RgbDefaultConfig {
    /// QMK's defaults, used when `[rgb_matrix.default]` is absent.
    fn default() -> Self {
        Self {
            on: default_on(),
            animation: default_animation(),
            hue: 0,
            sat: default_sat(),
            val: None,
            speed: default_speed(),
            flags: default_flags(),
        }
    }
}

/// The `[rgb_matrix]` section of `rgb.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RgbMatrixConfig {
    pub driver: RgbDriver,
    /// Chain length. Must agree with `[[rgb_matrix.layout]]` when both are set.
    pub led_count: Option<u16>,
    /// Ceiling for every brightness the host or a keycode can set
    /// (`RGB_MATRIX_MAXIMUM_BRIGHTNESS`). Also the scale Vial shows.
    #[serde(default = "default_max_brightness")]
    pub max_brightness: u8,
    /// Milliseconds without a key press after which the chain goes dark, `0`
    /// to never (`RGB_MATRIX_TIMEOUT`).
    #[serde(default)]
    pub timeout: u32,
    /// Milliseconds between chain updates (`RGB_MATRIX_LED_FLUSH_LIMIT`).
    #[serde(default = "default_led_flush_limit")]
    pub led_flush_limit: u16,
    /// Whether reactive effects answer key releases instead of presses
    /// (`RGB_MATRIX_KEYRELEASES`).
    #[serde(default)]
    pub react_on_keyup: bool,
    /// Geometric centre of the keyboard, used by pinwheel, spiral and beacon
    /// effects (`RGB_MATRIX_CENTER`).
    #[serde(default = "default_center_point")]
    pub center_point: [u8; 2],
    /// Step applied by each hue keycode (`RGB_MATRIX_HUE_STEP`).
    #[serde(default = "default_hue_step")]
    pub hue_steps: u8,
    /// Step applied by each saturation keycode (`RGB_MATRIX_SAT_STEP`).
    #[serde(default = "default_sat_step")]
    pub sat_steps: u8,
    /// Step applied by each brightness keycode (`RGB_MATRIX_VAL_STEP`).
    #[serde(default = "default_val_step")]
    pub val_steps: u8,
    /// Step applied by each speed keycode (`RGB_MATRIX_SPD_STEP`).
    #[serde(default = "default_speed_step")]
    pub speed_steps: u8,
    #[serde(default)]
    pub default: RgbDefaultConfig,
    /// Which effects are compiled in, keyed by effect name. An effect that is
    /// not listed does not exist in the firmware and is not offered to Vial.
    #[serde(default)]
    pub animations: BTreeMap<String, bool>,
    /// Every LED of the chain, in chain order.
    #[serde(default)]
    pub layout: Vec<LedConfig>,
}

/// Resolved RGB configuration, shared by code generation and the runtime.
#[derive(Clone, Debug)]
pub struct RgbConfig {
    pub driver: RgbDriver,
    pub ws2812: Ws2812Config,
    pub led_count: u16,
    pub max_brightness: u8,
    pub timeout_ms: u32,
    pub frame_ms: u16,
    pub react_on_keyup: bool,
    pub center: [u8; 2],
    pub hue_steps: u8,
    pub sat_steps: u8,
    pub val_steps: u8,
    pub speed_steps: u8,
    pub default: RgbDefault,
    pub animations: BTreeMap<String, bool>,
    pub layout: Vec<LedConfig>,
}

/// Resolved `[rgb_matrix.default]`, with `val` already defaulted.
#[derive(Clone, Debug)]
pub struct RgbDefault {
    pub on: bool,
    /// Effect name, validated by `rmk-macro` against the effect catalogue.
    pub animation: String,
    pub hue: u8,
    pub sat: u8,
    pub val: u8,
    pub speed: u8,
    pub flags: u8,
}

impl crate::KeyboardTomlConfig {
    /// Resolve the RGB configuration, or `None` when the keyboard has none.
    pub(crate) fn get_rgb_config(&self) -> Result<Option<RgbConfig>, String> {
        let (matrix, ws2812) = match (&self.rgb_matrix, &self.ws2812) {
            (None, None) => return Ok(None),
            (Some(_), None) => {
                return Err("rgb.toml: [rgb_matrix] needs a [ws2812] section for its data pin".to_string());
            }
            (None, Some(_)) => {
                return Err("rgb.toml: [ws2812] needs a [rgb_matrix] section to drive".to_string());
            }
            (Some(matrix), Some(ws2812)) => (matrix, ws2812),
        };

        if matrix.layout.is_empty() {
            return Err("rgb.toml: [[rgb_matrix.layout]] is required, one entry per LED in chain order".to_string());
        }
        let layout_len = matrix.layout.len() as u16;
        if let Some(led_count) = matrix.led_count
            && led_count != layout_len
        {
            return Err(format!(
                "rgb.toml: rgb_matrix.led_count is {led_count}, but [[rgb_matrix.layout]] has {layout_len} entries"
            ));
        }
        if matrix.max_brightness == 0 {
            return Err("rgb.toml: rgb_matrix.max_brightness must be greater than zero".to_string());
        }

        for (index, led) in matrix.layout.iter().enumerate() {
            if led.x > 224 || led.y > 64 {
                return Err(format!(
                    "rgb.toml: [[rgb_matrix.layout]] #{index} is at ({}, {}), outside QMK's 0..=224 by 0..=64 \
                     coordinate space that the animations assume",
                    led.x, led.y
                ));
            }
            if let (Some([row, col]), Some(layout)) = (led.matrix, self.layout.as_ref())
                && (row >= layout.rows || col >= layout.cols)
            {
                return Err(format!(
                    "rgb.toml: [[rgb_matrix.layout]] #{index} sits at matrix ({row}, {col}), outside the {}x{} \
                     matrix in keyboard.toml",
                    layout.rows, layout.cols
                ));
            }
            if matrix.layout[..index].iter().any(|other| other.matrix.is_some() && other.matrix == led.matrix) {
                // Multiple LEDs on one key are legitimate, but the same LED
                // listed twice is always a copy-paste slip.
                return Err(format!(
                    "rgb.toml: [[rgb_matrix.layout]] #{index} repeats matrix position {:?}",
                    led.matrix.unwrap()
                ));
            }
        }

        let default = &matrix.default;
        let rgb = RgbConfig {
            driver: matrix.driver,
            ws2812: ws2812.clone(),
            led_count: layout_len,
            max_brightness: matrix.max_brightness,
            timeout_ms: matrix.timeout,
            frame_ms: matrix.led_flush_limit,
            react_on_keyup: matrix.react_on_keyup,
            center: matrix.center_point,
            hue_steps: matrix.hue_steps,
            sat_steps: matrix.sat_steps,
            val_steps: matrix.val_steps,
            speed_steps: matrix.speed_steps,
            default: RgbDefault {
                on: default.on,
                animation: default.animation.clone(),
                hue: default.hue,
                sat: default.sat,
                val: default.val.unwrap_or(matrix.max_brightness),
                speed: default.speed,
                flags: default.flags,
            },
            animations: matrix.animations.clone(),
            layout: matrix.layout.clone(),
        };
        Ok(Some(rgb))
    }
}

fn default_color_order() -> ColorOrder {
    ColorOrder::Grb
}

fn default_timing_ns() -> u16 {
    1250
}

fn default_t1h_ns() -> u16 {
    900
}

fn default_t0h_ns() -> u16 {
    350
}

fn default_reset_us() -> u16 {
    280
}

fn default_max_brightness() -> u8 {
    u8::MAX
}

fn default_led_flush_limit() -> u16 {
    16
}

fn default_center_point() -> [u8; 2] {
    [112, 32]
}

fn default_hue_step() -> u8 {
    8
}

fn default_sat_step() -> u8 {
    16
}

fn default_val_step() -> u8 {
    16
}

fn default_speed_step() -> u8 {
    16
}

fn default_on() -> bool {
    true
}

fn default_animation() -> String {
    "solid_color".to_string()
}

fn default_sat() -> u8 {
    u8::MAX
}

fn default_speed() -> u8 {
    127
}

fn default_flags() -> u8 {
    u8::MAX
}
