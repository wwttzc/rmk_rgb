//! Per-key RGB lighting code generation, driven by `rgb.toml`.
//!
//! Two things come out of here: the board's [`LightingConfig`] tables, which
//! are the only thing `rgb.toml` is allowed to change, and the LED driver plus
//! the rendering task that consume them.
//!
//! Anything `rgb.toml` asks for that this firmware cannot do fails the build.
//! A quiet no-op would leave a name in the Vial panel that lights nothing up,
//! which is worse than a compile error naming the key.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use rmk_config::resolved::hardware::{ChipModel, ColorOrder, RgbConfig};
use rmk_types::lighting;

use super::input_device::Initializer;

/// Expand the `rgb.toml` configuration into initialization code and the
/// rendering task.
pub(crate) fn expand_rgb_config(chip: &ChipModel, rgb: &RgbConfig) -> (TokenStream, Initializer) {
    let modes = enabled_modes(rgb);
    let default_mode = resolve_default_mode(rgb, &modes);

    if rgb.react_on_keyup && !modes.iter().any(|&id| is_reactive(id)) {
        panic!(
            "rgb.toml: rgb_matrix.react_on_keyup = true, but no reactive effect is enabled. \
             Enable one under [rgb_matrix.animations] (solid_reactive_simple, splash, …) or drop the key."
        );
    }

    // The heatmap and rain effects keep one byte per matrix cell in fixed-size
    // statics, so a bigger matrix has nowhere to go.
    if rgb.matrix_rows as usize > lighting::MAX_MATRIX_ROWS
        || rgb.matrix_cols as usize > lighting::MAX_MATRIX_COLS
    {
        panic!(
            "rgb.toml: the keyboard matrix is {}x{}, but the framebuffer effects (typing_heatmap, \
             digital_rain) support at most {}x{}.",
            rgb.matrix_rows,
            rgb.matrix_cols,
            lighting::MAX_MATRIX_ROWS,
            lighting::MAX_MATRIX_COLS
        );
    }

    let led_count = rgb.led_count as usize;
    let points = rgb.layout.iter().map(|led| {
        let (x, y) = (led.x, led.y);
        quote! { (#x, #y) }
    });
    let flags = rgb.layout.iter().map(|led| led.flags);
    let matrix = rgb.layout.iter().map(|led| match led.matrix {
        Some([row, col]) => quote! { Some((#row, #col)) },
        None => quote! { None },
    });

    let modes_len = modes.len();
    // Vial's own table starts at its OFF id, so that is what the panel sees first.
    let vial_modes: Vec<u16> = std::iter::once(0)
        .chain(
            modes
                .iter()
                .copied()
                .filter(|&id| id <= lighting::LAST_VIAL_ID),
        )
        .collect();
    let vial_modes_len = vial_modes.len();

    let (max_brightness, frame_ms, timeout_ms) = (rgb.max_brightness, rgb.frame_ms, rgb.timeout_ms);
    let (center_x, center_y) = (rgb.center[0], rgb.center[1]);
    let react_on_keyup = rgb.react_on_keyup;
    let sleep = rgb.sleep;
    let (matrix_rows, matrix_cols) = (rgb.matrix_rows, rgb.matrix_cols);
    let (hue_steps, sat_steps, val_steps, speed_steps) =
        (rgb.hue_steps, rgb.sat_steps, rgb.val_steps, rgb.speed_steps);
    let (default_on, default_hue, default_sat, default_val, default_speed, default_flags) = (
        rgb.default.on,
        rgb.default.hue,
        rgb.default.sat,
        rgb.default.val,
        rgb.default.speed,
        rgb.default.flags,
    );

    let driver = expand_driver(chip, rgb, led_count);

    let initialization = quote! {
        const RGB_LED_COUNT: usize = #led_count;
        static RGB_POINTS: [(u8, u8); #led_count] = [#(#points),*];
        static RGB_FLAGS: [u8; #led_count] = [#(#flags),*];
        static RGB_MATRIX: [Option<(u8, u8)>; #led_count] = [#(#matrix),*];
        static RGB_MODES: [u16; #modes_len] = [#(#modes),*];
        static RGB_VIAL_MODES: [u16; #vial_modes_len] = [#(#vial_modes),*];
        static LIGHTING_CONFIG: ::rmk::lighting::LightingConfig = ::rmk::lighting::LightingConfig {
            max_brightness: #max_brightness,
            frame_ms: #frame_ms,
            timeout_ms: #timeout_ms,
            center: (#center_x, #center_y),
            hue_steps: #hue_steps,
            sat_steps: #sat_steps,
            val_steps: #val_steps,
            speed_steps: #speed_steps,
            react_on_keyup: #react_on_keyup,
            sleep: #sleep,
            default_on: #default_on,
            default_mode: #default_mode,
            default_hue: #default_hue,
            default_sat: #default_sat,
            default_val: #default_val,
            default_speed: #default_speed,
            default_flags: #default_flags,
            modes: &RGB_MODES,
            vial_modes: &RGB_VIAL_MODES,
            points: &RGB_POINTS,
            flags: &RGB_FLAGS,
            matrix: &RGB_MATRIX,
            matrix_rows: #matrix_rows,
            matrix_cols: #matrix_cols,
        };
        #driver
    };

    let processor = Initializer {
        initializer: quote! {},
        var_name: format_ident!("rgb_processor"),
    };

    (initialization, processor)
}

/// The enabled effects, as Vial ids in ascending order.
///
/// Solid colour is always included: QMK gives it no enable flag at all, so a
/// board cannot ship without a plain colour to fall back on.
fn enabled_modes(rgb: &RgbConfig) -> Vec<u16> {
    let mut modes: Vec<u16> = Vec::new();
    for (name, enabled) in &rgb.animations {
        if !enabled {
            continue;
        }
        if name == "off" {
            panic!(
                "rgb.toml: [rgb_matrix.animations].off is not an effect — being off is \
                 [rgb_matrix.default].on = false, or the OFF entry in Vial's panel."
            );
        }
        let effect = lighting::effect_by_name(name).unwrap_or_else(|| {
            let known: Vec<&str> = lighting::EFFECTS
                .iter()
                .filter(|effect| effect.implemented())
                .map(|effect| effect.name)
                .collect();
            panic!(
                "rgb.toml: [rgb_matrix.animations] has no effect called `{name}`. \
                 Enabled effects are: {}.",
                known.join(", ")
            );
        });
        if !effect.implemented() {
            panic!(
                "rgb.toml: the effect `{name}` is part of QMK's RGB Matrix but RMK cannot render \
                 it yet, so enabling it would put a dead entry in Vial's panel."
            );
        }
        modes.push(effect.id);
    }
    // Solid colour carries no `ENABLE_` flag in QMK, so it is always compiled in.
    modes.push(2);
    modes.sort_unstable();
    modes.dedup();
    modes
}

/// Resolve `[rgb_matrix.default].animation` to a mode id, checking it is on.
fn resolve_default_mode(rgb: &RgbConfig, modes: &[u16]) -> u16 {
    let name = rgb.default.animation.as_str();
    let effect = lighting::effect_by_name(name).unwrap_or_else(|| {
        let known: Vec<&str> = lighting::EFFECTS
            .iter()
            .filter(|effect| effect.implemented())
            .map(|effect| effect.name)
            .collect();
        panic!(
            "rgb.toml: [rgb_matrix.default].animation = \"{name}\" is not an effect name. \
             Enabled effects are: {}.",
            known.join(", ")
        )
    });
    if !effect.implemented() {
        panic!(
            "rgb.toml: [rgb_matrix.default].animation = \"{name}\" is part of QMK's RGB Matrix \
             but RMK cannot render it yet."
        );
    }
    if !modes.contains(&effect.id) {
        panic!(
            "rgb.toml: [rgb_matrix.default].animation = \"{name}\" is not enabled under \
             [rgb_matrix.animations], so the firmware has no way to render it."
        );
    }
    effect.id
}

/// Whether an effect answers key events, which is what `react_on_keyup` needs.
fn is_reactive(id: u16) -> bool {
    lighting::effect_by_id(id).is_some_and(|effect| effect.is_reactive())
}

/// The LED driver, which today only exists for the ESP32-S3's RMT peripheral.
fn expand_driver(chip: &ChipModel, rgb: &RgbConfig, led_count: usize) -> TokenStream {
    if chip.chip != "esp32s3" {
        panic!(
            "rgb.toml: per-key RGB drives a WS2812 chain from the chip's RMT peripheral, and \
             RMK only implements that for esp32s3. This keyboard is `{}`.",
            chip.chip
        );
    }
    let pin_name = rgb.ws2812.pin.as_str();
    if !pin_name.starts_with("GPIO") {
        panic!(
            "rgb.toml: [ws2812].pin = \"{pin_name}\" is not an ESP32-S3 pin name; write it like \
             the pins in keyboard.toml, for example \"GPIO4\"."
        );
    }
    let pin = format_ident!("{}", pin_name);
    let order = match rgb.ws2812.color_order {
        ColorOrder::Rgb => format_ident!("Rgb"),
        ColorOrder::Grb => format_ident!("Grb"),
        ColorOrder::Bgr => format_ident!("Bgr"),
        ColorOrder::Rgbw => format_ident!("Rgbw"),
        ColorOrder::Grbw => format_ident!("Grbw"),
    };
    let ws2812 = &rgb.ws2812;
    let (t0h, t0l, t1h, t1l) = (
        ws2812.t0h_ns,
        ws2812.timing_ns.saturating_sub(ws2812.t0h_ns),
        ws2812.t1h_ns,
        ws2812.timing_ns.saturating_sub(ws2812.t1h_ns),
    );
    let reset_us = ws2812.reset_us;

    quote! {
        // One RMT channel drives the whole chain, at the RMT's 80 MHz clock.
        let rmt_freq = ::esp_hal::time::Rate::from_mhz(80);
        let rmt = ::esp_hal::rmt::Rmt::new(p.RMT, rmt_freq).unwrap();
        let rgb_leds = ::esp_hal_smartled::RmtSmartLeds::<
            { ::esp_hal_smartled::buffer_size::<::smart_leds_trait::RGB<u8>>(#led_count) },
            _,
            ::smart_leds_trait::RGB<u8>,
            ::esp_hal_smartled::color_order::#order,
        >::new(
            ::esp_hal_smartled::Timing {
                time_0_high: #t0h,
                time_0_low: #t0l,
                time_1_high: #t1h,
                time_1_low: #t1l,
                reset_us: #reset_us,
            },
            rmt.channel0,
            p.#pin,
            rmt_freq,
        )
        .unwrap();
        let mut rgb_processor = ::rmk::lighting::LightingProcessor::new(rgb_leds, &LIGHTING_CONFIG);
    }
}

#[cfg(test)]
mod tests {
    use rmk_config::resolved::hardware::{
        ChipSeries, LedConfig, RgbConfig, RgbDefault, RgbDriver, Ws2812Config,
    };

    use super::*;

    fn chain(animations: &[(&str, bool)], animation: &str) -> RgbConfig {
        RgbConfig {
            driver: RgbDriver::Ws2812,
            ws2812: Ws2812Config {
                pin: "GPIO4".to_string(),
                color_order: ColorOrder::Grb,
                timing_ns: 1250,
                t1h_ns: 900,
                t0h_ns: 350,
                reset_us: 280,
            },
            led_count: 1,
            max_brightness: 128,
            timeout_ms: 0,
            frame_ms: 33,
            react_on_keyup: false,
            sleep: false,
            center: [112, 32],
            hue_steps: 8,
            sat_steps: 16,
            val_steps: 16,
            speed_steps: 16,
            default: RgbDefault {
                on: true,
                animation: animation.to_string(),
                hue: 0,
                sat: 255,
                val: 64,
                speed: 128,
                flags: 255,
            },
            animations: animations
                .iter()
                .map(|(name, on)| (name.to_string(), *on))
                .collect(),
            layout: vec![LedConfig {
                matrix: Some([0, 0]),
                x: 0,
                y: 0,
                flags: 4,
            }],
            matrix_rows: 2,
            matrix_cols: 2,
        }
    }

    #[test]
    fn enabled_effects_become_mode_ids_in_order() {
        let modes = enabled_modes(&chain(
            &[("cycle_all", true), ("breathing", true)],
            "breathing",
        ));
        assert_eq!(
            modes,
            vec![2, 6, 13],
            "sorted, and solid colour is always there"
        );
    }

    #[test]
    fn a_disabled_effect_is_not_compiled_in() {
        let modes = enabled_modes(&chain(
            &[("cycle_all", false), ("breathing", true)],
            "breathing",
        ));
        assert_eq!(modes, vec![2, 6]);
    }

    #[test]
    #[should_panic(expected = "has no effect called `brerthing`")]
    fn a_misspelled_effect_is_rejected() {
        enabled_modes(&chain(&[("brerthing", true)], "breathing"));
    }

    #[test]
    #[should_panic(expected = "is not an effect")]
    fn off_is_not_an_animation() {
        enabled_modes(&chain(&[("off", true)], "solid_color"));
    }

    #[test]
    #[should_panic(expected = "not enabled under")]
    fn the_default_effect_has_to_be_enabled() {
        let rgb = chain(&[("breathing", true)], "cycle_all");
        resolve_default_mode(&rgb, &enabled_modes(&rgb));
    }

    #[test]
    fn the_default_effect_resolves_to_its_id() {
        let rgb = chain(&[("breathing", true)], "breathing");
        assert_eq!(resolve_default_mode(&rgb, &enabled_modes(&rgb)), 6);
    }

    fn esp32s3() -> ChipModel {
        ChipModel {
            series: ChipSeries::Esp32,
            chip: "esp32s3".to_string(),
            board: None,
        }
    }

    /// The expansion is only ever compiled on the target, so nothing here would
    /// catch a malformed token stream until CI does. Parsing it as Rust does.
    #[test]
    fn the_generated_code_parses_as_rust() {
        let rgb = chain(
            &[("breathing", true), ("typing_heatmap", true)],
            "breathing",
        );
        let (init, processor) = expand_rgb_config(&esp32s3(), &rgb);
        syn::parse2::<syn::Block>(quote! { { #init } })
            .expect("the lighting initialization must be valid Rust");
        assert_eq!(processor.var_name.to_string(), "rgb_processor");
    }

    /// Point `RMK_RGB_CHECK_KEYBOARD_TOML` at a real `keyboard.toml` to expand
    /// its `rgb.toml` too, which is how a board's driver code is checked before
    /// it reaches CI.
    #[test]
    fn a_real_board_configuration_expands() {
        let Ok(path) = std::env::var("RMK_RGB_CHECK_KEYBOARD_TOML") else {
            return;
        };
        let config = rmk_config::KeyboardTomlConfig::new_from_toml_path(&path);
        let hardware = config
            .hardware()
            .expect("the hardware configuration must resolve");
        let rgb = hardware
            .rgb
            .expect("rgb.toml must be next to keyboard.toml");
        let (init, _) = expand_rgb_config(&hardware.chip, &rgb);
        println!("{init}");
        syn::parse2::<syn::Block>(quote! { { #init } })
            .expect("the lighting initialization must be valid Rust");
    }
}
