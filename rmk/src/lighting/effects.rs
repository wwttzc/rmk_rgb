//! The animations, ported from QMK's `quantum/rgb_matrix/animations`.
//!
//! Each function is the body of one QMK effect, and the C implicit conversions
//! are kept explicit: `hsv.h = x + y` wraps in `u8` here just as it does there,
//! and arguments C truncates into a byte (everything reaching `abs8` or
//! `scale8`) are truncated here the same way.
//!
//! Where QMK's effect is a thin wrapper around a shared runner
//! (`effect_runner_i`, `effect_runner_dx_dy`, `effect_runner_dx_dy_dist`,
//! `effect_runner_sin_cos_i`), the Rust version keeps the same shape, so an
//! effect can be read next to its original.
//!
//! One deliberate difference: QMK renders a frame in chunks of
//! `RGB_MATRIX_LED_PROCESS_LIMIT` LEDs, so an effect can be called several
//! times per frame and its state updates are guarded on `params->iter`. RMK
//! renders the whole chain in one pass, which is what QMK itself does when
//! `led_process_limit` covers the chain, so per-frame state updates happen
//! exactly once per frame.

use rmk_types::lighting::{MAX_MATRIX_COLS, MAX_MATRIX_ROWS};

use crate::lighting::color::{Hsv, Rgb, hsv_to_rgb};
use crate::lighting::effect::{Effect, EffectCtx, EffectState};
use crate::lighting::lib8tion::{Rand16, abs8, atan2_8, cos8, qadd8, qsub8, scale8, scale16by8, sin8, sqrt16};
use crate::lighting::{LightingConfig, MAX_LEDS};

/// `LED_FLAG_MODIFIER`, the one per-LED flag an effect reads for semantics.
const LED_FLAG_MODIFIER: u8 = 0x01;

/// Render one frame of `effect` into `frame`.
pub fn render(effect: Effect, ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    match effect {
        Effect::Off => frame.fill(Rgb::BLACK),
        Effect::Direct => direct(ctx, frame),
        Effect::SolidColor => solid_color(ctx, frame),
        Effect::AlphasMods => alpha_mods(ctx, frame),
        Effect::GradientUpDown => gradient_up_down(ctx, frame),
        Effect::GradientLeftRight => gradient_left_right(ctx, frame),
        Effect::Breathing => breathing(ctx, frame),
        Effect::BandSat => band_sat(ctx, frame),
        Effect::BandVal => band_val(ctx, frame),
        Effect::BandPinwheelSat => band_pinwheel_sat(ctx, frame),
        Effect::BandPinwheelVal => band_pinwheel_val(ctx, frame),
        Effect::BandSpiralSat => band_spiral_sat(ctx, frame),
        Effect::BandSpiralVal => band_spiral_val(ctx, frame),
        Effect::CycleAll => cycle_all(ctx, frame),
        Effect::CycleLeftRight => cycle_left_right(ctx, frame),
        Effect::CycleUpDown => cycle_up_down(ctx, frame),
        Effect::RainbowMovingChevron => rainbow_moving_chevron(ctx, frame),
        Effect::CycleOutIn => cycle_out_in(ctx, frame),
        Effect::CycleOutInDual => cycle_out_in_dual(ctx, frame),
        Effect::CyclePinwheel => cycle_pinwheel(ctx, frame),
        Effect::CycleSpiral => cycle_spiral(ctx, frame),
        Effect::DualBeacon => dual_beacon(ctx, frame),
        Effect::RainbowBeacon => rainbow_beacon(ctx, frame),
        Effect::RainbowPinwheels => rainbow_pinwheels(ctx, frame),
        Effect::Raindrops => raindrops(ctx, frame),
        Effect::JellybeanRaindrops => jellybean_raindrops(ctx, frame),
        Effect::HueBreathing => hue_breathing(ctx, frame),
        Effect::HuePendulum => hue_pendulum(ctx, frame),
        Effect::HueWave => hue_wave(ctx, frame),
        Effect::PixelRain => pixel_rain(ctx, frame),
        Effect::PixelFlow => pixel_flow(ctx, frame),
        Effect::StarlightSmooth => starlight_smooth(ctx, frame),
        Effect::FlowerBlooming => flower_blooming(ctx, frame),
        Effect::Riverflow => riverflow(ctx, frame),
        Effect::Starlight => starlight(ctx, frame),
        Effect::StarlightDualSat => starlight_dual_sat(ctx, frame),
        Effect::StarlightDualHue => starlight_dual_hue(ctx, frame),
        Effect::TypingHeatmap => typing_heatmap(ctx, frame),
        Effect::DigitalRain => digital_rain(ctx, frame),
        Effect::SolidReactiveSimple => solid_reactive_simple(ctx, frame),
        Effect::SolidReactive => solid_reactive(ctx, frame),
        Effect::SolidReactiveWide => reactive_splash(ctx, frame, ReactiveShape::Wide, false),
        Effect::SolidReactiveMultiwide => reactive_splash(ctx, frame, ReactiveShape::Wide, true),
        Effect::SolidReactiveCross => reactive_splash(ctx, frame, ReactiveShape::Cross, false),
        Effect::SolidReactiveMulticross => reactive_splash(ctx, frame, ReactiveShape::Cross, true),
        Effect::SolidReactiveNexus => reactive_splash(ctx, frame, ReactiveShape::Nexus, false),
        Effect::SolidReactiveMultinexus => reactive_splash(ctx, frame, ReactiveShape::Nexus, true),
        Effect::Splash => reactive_splash(ctx, frame, ReactiveShape::Splash, false),
        Effect::Multisplash => reactive_splash(ctx, frame, ReactiveShape::Splash, true),
        Effect::SolidSplash => reactive_splash(ctx, frame, ReactiveShape::SolidSplash, false),
        Effect::SolidMultisplash => reactive_splash(ctx, frame, ReactiveShape::SolidSplash, true),
        Effect::PixelFractal => pixel_fractal(ctx, frame),
    }
}

/// Set every LED the flag mask selects, leaving the others as they were.
fn each_led(cfg: &LightingConfig, flags: u8, frame: &mut [Rgb], mut f: impl FnMut(usize) -> Rgb) {
    for (led, colour) in frame.iter_mut().enumerate() {
        if cfg.flags[led] & flags == 0 {
            continue;
        }
        *colour = f(led);
    }
}

/// A hue shift applied the way QMK's `hsv.h += delta` applies it: the sum is
/// truncated into a byte, so a negative shift wraps.
fn shift_hue(hsv: Hsv, delta: i32) -> Hsv {
    Hsv {
        h: hsv.h.wrapping_add(delta as u8),
        ..hsv
    }
}

/// The brightness envelope shared by every breathing-shaped effect:
/// `scale8(abs8(sin8(theta) - 128) * 2, value)`.
fn breath_value(theta: u8, value: u8) -> u8 {
    let wave = abs8((sin8(theta) as i32 - 128) as i8);
    scale8((wave as i32 * 2) as u8, value)
}

// ---------------------------------------------------------------- solid / off

fn direct(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    for (led, colour) in frame.iter_mut().enumerate() {
        *colour = hsv_to_rgb(ctx.state.direct[led]);
    }
}

fn solid_color(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let colour = hsv_to_rgb(ctx.hsv);
    each_led(ctx.cfg, ctx.flags, frame, |_| colour);
}

fn alpha_mods(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let colour = hsv_to_rgb(ctx.hsv);
    let modifier_colour = hsv_to_rgb(shift_hue(ctx.hsv, ctx.speed as i32));
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        if ctx.cfg.flags[led] & LED_FLAG_MODIFIER != 0 {
            modifier_colour
        } else {
            colour
        }
    });
}

// ----------------------------------------------------------------- gradients

fn gradient_up_down(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let scale = scale8(64, ctx.speed);
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        // y spans 0..64, so this maps onto 0..4 hue steps.
        hsv_to_rgb(shift_hue(ctx.hsv, scale as i32 * (ctx.cfg.points[led].1 >> 4) as i32))
    });
}

fn gradient_left_right(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let scale = scale8(64, ctx.speed);
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        // x spans 0..224, so this maps onto 0..7 hue steps.
        hsv_to_rgb(shift_hue(ctx.hsv, (scale as i32 * ctx.cfg.points[led].0 as i32) >> 5))
    });
}

// ---------------------------------------------------------------------- bands

fn band_sat(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let delta = (scale8(ctx.cfg.points[led].0, 228) as i32 + 28 - time as i32).abs() * 8;
        hsv_to_rgb(Hsv {
            s: EffectCtx::band(ctx.hsv.s, delta),
            ..ctx.hsv
        })
    });
}

fn band_val(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let delta = (scale8(ctx.cfg.points[led].0, 228) as i32 + 28 - time as i32).abs() * 8;
        hsv_to_rgb(Hsv {
            v: EffectCtx::band(ctx.hsv.v, delta),
            ..ctx.hsv
        })
    });
}

fn band_pinwheel_sat(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        let s = ctx.hsv.s as i32 - time as i32 - atan2_8(dy, dx) as i32 * 3;
        hsv_to_rgb(Hsv {
            s: scale8(s as u8, ctx.hsv.s),
            ..ctx.hsv
        })
    });
}

fn band_pinwheel_val(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        let v = ctx.hsv.v as i32 - time as i32 - atan2_8(dy, dx) as i32 * 3;
        hsv_to_rgb(Hsv {
            v: scale8(v as u8, ctx.hsv.v),
            ..ctx.hsv
        })
    });
}

fn band_spiral_sat(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        let s = ctx.hsv.s as i32 + ctx.dist(led) as i32 - time as i32 - atan2_8(dy, dx) as i32;
        hsv_to_rgb(Hsv {
            s: scale8(s as u8, ctx.hsv.s),
            ..ctx.hsv
        })
    });
}

fn band_spiral_val(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        let v = ctx.hsv.v as i32 + ctx.dist(led) as i32 - time as i32 - atan2_8(dy, dx) as i32;
        hsv_to_rgb(Hsv {
            v: scale8(v as u8, ctx.hsv.v),
            ..ctx.hsv
        })
    });
}

// ---------------------------------------------------------------------- cycle

fn cycle_all(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    let colour = hsv_to_rgb(Hsv { h: time, ..ctx.hsv });
    each_led(ctx.cfg, ctx.flags, frame, |_| colour);
}

fn cycle_left_right(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        hsv_to_rgb(Hsv {
            h: ctx.cfg.points[led].0.wrapping_sub(time),
            ..ctx.hsv
        })
    });
}

fn cycle_up_down(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        hsv_to_rgb(Hsv {
            h: ctx.cfg.points[led].1.wrapping_sub(time),
            ..ctx.hsv
        })
    });
}

fn rainbow_moving_chevron(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (x, y) = ctx.cfg.points[led];
        let wave = abs8((y as i32 - ctx.cfg.center.1 as i32) as i8) as i32 + (x as i32 - time as i32);
        hsv_to_rgb(shift_hue(ctx.hsv, wave))
    });
}

fn cycle_out_in(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        hsv_to_rgb(Hsv {
            h: (3 * ctx.dist(led) as i32 / 2 + time as i32) as u8,
            ..ctx.hsv
        })
    });
}

fn cycle_out_in_dual(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        // Half the centre x folds the right half onto the left one, as QMK does
        // by writing the literal 56 for its default centre.
        let dx = ctx.cfg.center.0 as i32 / 2 - abs8(dx as i8) as i32;
        let dist = crate::lighting::lib8tion::sqrt16((dx * dx + dy as i32 * dy as i32) as u16);
        hsv_to_rgb(Hsv {
            h: (3 * dist as i32 + time as i32) as u8,
            ..ctx.hsv
        })
    });
}

fn cycle_pinwheel(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        hsv_to_rgb(Hsv {
            h: atan2_8(dy, dx).wrapping_add(time),
            ..ctx.hsv
        })
    });
}

fn cycle_spiral(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        hsv_to_rgb(Hsv {
            h: ctx.dist(led).wrapping_sub(time).wrapping_sub(atan2_8(dy, dx)),
            ..ctx.hsv
        })
    });
}

// -------------------------------------------------------------------- beacons

/// The three beacon effects differ only in how they scale the two terms of
/// `(dy * ky * cos + fx * kx * sin) / 128`.
fn beacon(ctx: &mut EffectCtx, frame: &mut [Rgb], y_scale: i32, x_scale: i32, fold_x: bool) {
    let time = ctx.time_sin_cos();
    // `int8_t cos_value = cos8(time) - 128` in QMK: the difference fits an i8.
    let cos_value = (cos8(time as u8) as i32 - 128) as i8 as i32;
    let sin_value = (sin8(time as u8) as i32 - 128) as i8 as i32;
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (dx, dy) = ctx.dx_dy(led);
        let dx = dx as i32;
        let term = if fold_x {
            let folded = 56 - abs8(dx as i8) as i32;
            (dy as i32 * y_scale * cos_value + folded * x_scale * sin_value) / 128
        } else {
            (dy as i32 * y_scale * cos_value + dx * x_scale * sin_value) / 128
        };
        hsv_to_rgb(shift_hue(ctx.hsv, term))
    });
}

fn dual_beacon(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    beacon(ctx, frame, 1, 1, false);
}

fn rainbow_beacon(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    beacon(ctx, frame, 2, 2, false);
}

fn rainbow_pinwheels(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    beacon(ctx, frame, 3, 3, true);
}

// ------------------------------------------------------------------ breathing

fn breathing(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    let colour = hsv_to_rgb(Hsv {
        v: breath_value(time / 2, ctx.hsv.v),
        ..ctx.hsv
    });
    each_led(ctx.cfg, ctx.flags, frame, |_| colour);
}

fn hue_breathing(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    let delta = scale8((abs8((sin8(time / 2) as i32 - 128) as i8) as i32 * 2) as u8, 12);
    let colour = hsv_to_rgb(shift_hue(ctx.hsv, delta as i32));
    each_led(ctx.cfg, ctx.flags, frame, |_| colour);
}

fn hue_pendulum(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let wave = (sin8(time) as i32 + ctx.cfg.points[led].0 as i32 - 128) as i8;
        let delta = scale8((abs8(wave) as i32 * 2) as u8, 12);
        hsv_to_rgb(shift_hue(ctx.hsv, delta as i32))
    });
}

fn hue_wave(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let wave = (ctx.cfg.points[led].0 as i32 - time as i32) as i8;
        hsv_to_rgb(shift_hue(ctx.hsv, scale8(abs8(wave) as u8, 24) as i32))
    });
}

// ------------------------------------------------------------------ raindrops

/// RAINDROPS' colour for one LED: a quarter of the way towards the opposite
/// hue, taken up to twice.
fn raindrops_colour(rand: &mut Rand16, hsv: Hsv) -> Rgb {
    let delta_h = (hsv.h.wrapping_add(128) as i8).wrapping_sub(hsv.h as i8) / 4;
    hsv_to_rgb(shift_hue(hsv, delta_h as i32 * rand.random8_max(3) as i32))
}

/// The colour jellybean-style effects give a freshly lit LED.
fn jellybean_colour(rand: &mut Rand16, v: u8) -> Rgb {
    hsv_to_rgb(Hsv {
        h: rand.random8(),
        s: rand.random8_min_max(127, 255),
        v,
    })
}

fn raindrops(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    if scale16by8(ctx.timer as u16, qadd8(ctx.speed, 16)).is_multiple_of(10) {
        ctx.state.raindrops_index = ctx.state.rand.random8_max(count as u8) as u16;
    }

    if ctx.init {
        each_led(ctx.cfg, ctx.flags, frame, |_| {
            raindrops_colour(&mut ctx.state.rand, ctx.hsv)
        });
        return;
    }
    if count > ctx.state.raindrops_index as usize && ctx.led_enabled(ctx.state.raindrops_index as usize) {
        let index = ctx.state.raindrops_index as usize;
        frame[index] = raindrops_colour(&mut ctx.state.rand, ctx.hsv);
        ctx.state.raindrops_index = count as u16 + 1;
    }
}

fn jellybean_raindrops(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    if scale16by8(ctx.timer as u16, qadd8(ctx.speed, 16)).is_multiple_of(5) {
        ctx.state.raindrops_index = ctx.state.rand.random8_max(count as u8) as u16;
    }

    if ctx.init {
        each_led(ctx.cfg, ctx.flags, frame, |_| {
            jellybean_colour(&mut ctx.state.rand, ctx.hsv.v)
        });
        return;
    }
    if count > ctx.state.raindrops_index as usize && ctx.led_enabled(ctx.state.raindrops_index as usize) {
        let index = ctx.state.raindrops_index as usize;
        frame[index] = jellybean_colour(&mut ctx.state.rand, ctx.hsv.v);
        ctx.state.raindrops_index = count as u16 + 1;
    }
}

// ------------------------------------------------------------------ starlight

fn starlight(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    if scale16by8(ctx.timer as u16, qadd8(ctx.speed, 5)).is_multiple_of(5) {
        ctx.state.starlight_index = ctx.state.rand.random8_max(count as u8) as u16;
    }
    let time = scale16by8(ctx.timer as u16, ctx.speed / 8);
    let v = breath_value(time as u8, ctx.hsv.v);

    if ctx.init {
        let colour = hsv_to_rgb(Hsv { v, ..ctx.hsv });
        each_led(ctx.cfg, ctx.flags, frame, |_| colour);
        return;
    }
    if count > ctx.state.starlight_index as usize && ctx.led_enabled(ctx.state.starlight_index as usize) {
        let index = ctx.state.starlight_index as usize;
        frame[index] = hsv_to_rgb(Hsv { v, ..ctx.hsv });
        ctx.state.starlight_index = count as u16 + 1;
    }
}

fn starlight_dual_sat(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    let (speed, timer, hsv) = (ctx.speed, ctx.timer, ctx.hsv);
    if scale16by8(timer as u16, qadd8(speed, 5)).is_multiple_of(5) {
        ctx.state.starlight_index = ctx.state.rand.random8_max(count as u8) as u16;
    }
    let time = scale16by8(timer as u16, speed / 8);
    let v = breath_value(time as u8, hsv.v);
    // The saturation jitter is per LED, so the colour is built where it is used.
    let colour = |rand: &mut Rand16| {
        hsv_to_rgb(Hsv {
            v,
            s: hsv.s.wrapping_add(rand.random8_max(31)),
            ..hsv
        })
    };

    if ctx.init {
        each_led(ctx.cfg, ctx.flags, frame, |_| colour(&mut ctx.state.rand));
        return;
    }
    if count > ctx.state.starlight_index as usize && ctx.led_enabled(ctx.state.starlight_index as usize) {
        let index = ctx.state.starlight_index as usize;
        frame[index] = colour(&mut ctx.state.rand);
        ctx.state.starlight_index = count as u16 + 1;
    }
}

fn starlight_dual_hue(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    let (speed, timer, hsv) = (ctx.speed, ctx.timer, ctx.hsv);
    if scale16by8(timer as u16, qadd8(speed, 5)).is_multiple_of(5) {
        ctx.state.starlight_index = ctx.state.rand.random8_max(count as u8) as u16;
    }
    let time = scale16by8(timer as u16, speed / 8);
    let v = breath_value(time as u8, hsv.v);
    let colour = |rand: &mut Rand16| {
        hsv_to_rgb(Hsv {
            v,
            h: hsv.h.wrapping_add(rand.random8_max(31)),
            ..hsv
        })
    };

    if ctx.init {
        each_led(ctx.cfg, ctx.flags, frame, |_| colour(&mut ctx.state.rand));
        return;
    }
    if count > ctx.state.starlight_index as usize && ctx.led_enabled(ctx.state.starlight_index as usize) {
        let index = ctx.state.starlight_index as usize;
        frame[index] = colour(&mut ctx.state.rand);
        ctx.state.starlight_index = count as u16 + 1;
    }
}

fn starlight_smooth(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let time = ctx.time_i();
    if ctx.init {
        ctx.state.starlight_phase = [0; MAX_LEDS];
    }
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        if ctx.state.starlight_phase[led] == 0 {
            ctx.state.starlight_phase[led] = ctx.state.rand.random8();
        }
        // `(time + phase) / 2` stays in `int` in QMK before `sin8` truncates it.
        let phase = ctx.state.starlight_phase[led];
        let theta = ((time as i32 + phase as i32) / 2) as u8;
        hsv_to_rgb(Hsv {
            v: breath_value(theta, ctx.hsv.v),
            ..ctx.hsv
        })
    });
}

// ---------------------------------------------------------------- pixel* set

fn pixel_rain(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    if ctx.init {
        ctx.state.pixel_rain_index = ctx.state.rand.random8_max(count as u8);
    }
    if ctx.state.pixel_rain_timer < ctx.timer {
        let index = ctx.state.pixel_rain_index as usize;
        if index < count && ctx.led_enabled(index) {
            frame[index] = if ctx.state.rand.random8() & 2 != 0 {
                Rgb::BLACK
            } else {
                jellybean_colour(&mut ctx.state.rand, ctx.hsv.v)
            };
        }
        ctx.state.pixel_rain_index = ctx.state.rand.random8_max(count as u8);
        ctx.state.pixel_rain_timer = ctx.timer + (2048 - scale16by8(1792, ctx.speed) as u32);
    }
}

fn pixel_flow(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let count = ctx.cfg.points.len();
    if ctx.init {
        // QMK blanks the chain first, so a LED this effect never touches does
        // not keep the previous effect's colour.
        frame.fill(Rgb::BLACK);
        for led in 0..count {
            ctx.state.pixel_flow[led] = if ctx.state.rand.random8() & 2 != 0 {
                Rgb::BLACK
            } else {
                jellybean_colour(&mut ctx.state.rand, ctx.hsv.v)
            };
        }
    }

    for (led, colour) in frame.iter_mut().enumerate().take(count) {
        if ctx.led_enabled(led) {
            *colour = ctx.state.pixel_flow[led];
        }
    }

    if ctx.state.pixel_flow_wait_timer <= ctx.timer {
        for led in 0..count.saturating_sub(1) {
            ctx.state.pixel_flow[led] = ctx.state.pixel_flow[led + 1];
        }
        let last = count - 1;
        ctx.state.pixel_flow[last] = if ctx.state.rand.random8() & 2 != 0 {
            Rgb::BLACK
        } else {
            jellybean_colour(&mut ctx.state.rand, ctx.hsv.v)
        };
        // 11 ms at full speed, 187 ms at zero.
        let interval = 3000 / scale16by8(qadd8(ctx.speed, 16) as u16, 16) as u32;
        ctx.state.pixel_flow_wait_timer = ctx.timer + interval;
    }
}

// --------------------------------------------------------------- one-offs

fn flower_blooming(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    // The only effect with a tenth-speed clock.
    let time = scale16by8(ctx.timer as u16, qadd8(ctx.speed / 10, 1));
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let (x, y) = ctx.cfg.points[led];
        let base = x as i32 * 3 - y as i32 * 3;
        let h = if y > ctx.cfg.center.1 {
            base + time as i32
        } else {
            base - time as i32
        };
        let rgb = hsv_to_rgb(Hsv { h: h as u8, ..ctx.hsv });
        if y > ctx.cfg.center.1 {
            Rgb::new(rgb.b, rgb.g, rgb.r)
        } else {
            rgb
        }
    });
}

fn riverflow(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let time = scale16by8(ctx.timer.wrapping_add(led as u32 * 315) as u16, ctx.speed / 8);
        hsv_to_rgb(Hsv {
            v: breath_value(time as u8, ctx.hsv.v),
            ..ctx.hsv
        })
    });
}

// ------------------------------------------------------------- reactive hits

/// QMK's `effect_runner_reactive`: every LED takes the age of the newest hit on
/// it, scaled by speed, and hands that offset to `f`.
fn reactive(ctx: &mut EffectCtx, frame: &mut [Rgb], f: impl Fn(Hsv, u16) -> Hsv) {
    let speed_factor = qadd8(ctx.speed, 1);
    let max_tick = u16::MAX / speed_factor as u16;
    let hits = ctx.state.hits;
    each_led(ctx.cfg, ctx.flags, frame, |led| {
        let tick = hits.newest_tick(led as u8, max_tick);
        hsv_to_rgb(f(ctx.hsv, scale16by8(tick, speed_factor)))
    });
}

fn solid_reactive_simple(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    reactive(ctx, frame, |hsv, offset| Hsv {
        v: scale8(255 - offset.min(255) as u8, hsv.v),
        ..hsv
    });
}

fn solid_reactive(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    reactive(ctx, frame, |hsv, offset| {
        shift_hue(hsv, scale8(255 - offset.min(255) as u8, 64) as i32)
    });
}

/// Which of QMK's splash-shaped reactive effects to draw.
#[derive(Clone, Copy)]
enum ReactiveShape {
    /// `solid_reactive_wide`
    Wide,
    /// `solid_reactive_cross`
    Cross,
    /// `solid_reactive_nexus`
    Nexus,
    /// `splash`
    Splash,
    /// `solid_splash`
    SolidSplash,
}

/// QMK's `effect_runner_reactive_splash`.
///
/// `multi` picks whether every remembered hit contributes or only the most
/// recent one, which is the whole difference between each pair of effects.
fn reactive_splash(ctx: &mut EffectCtx, frame: &mut [Rgb], shape: ReactiveShape, multi: bool) {
    let hits = ctx.state.hits;
    let start = if multi { 0 } else { qsub8(hits.count, 1) as usize };
    let speed_factor = qadd8(ctx.speed, 1);
    let base = ctx.hsv;

    each_led(ctx.cfg, ctx.flags, frame, |led| {
        // QMK starts from the configured colour with the brightness at zero and
        // lets each hit add to it, then scales the result by the brightness.
        let mut hsv = Hsv { v: 0, ..base };
        for slot in start..hits.count as usize {
            let (x, y) = ctx.cfg.points[led];
            let dx = x as i16 - hits.x[slot] as i16;
            let dy = y as i16 - hits.y[slot] as i16;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let tick = scale16by8(hits.tick[slot], speed_factor);
            hsv = match shape {
                ReactiveShape::Wide => {
                    // C computes this in `int` and stores it in a `uint16_t`, so
                    // it wraps rather than saturating before the clamp.
                    let effect = tick.wrapping_add((dist as u16).wrapping_mul(5)).min(255) as u8;
                    Hsv {
                        v: qadd8(hsv.v, 255 - effect),
                        ..hsv
                    }
                }
                ReactiveShape::Cross => {
                    let dx = (dx.unsigned_abs() * 16).min(255);
                    let dy = (dy.unsigned_abs() * 16).min(255);
                    let effect = tick.wrapping_add(dist as u16).wrapping_add(dx.min(dy)).min(255) as u8;
                    Hsv {
                        v: qadd8(hsv.v, 255 - effect),
                        ..hsv
                    }
                }
                ReactiveShape::Nexus => {
                    let mut effect = tick.wrapping_sub(dist as u16);
                    if effect > 255 || dist > 72 || (dx.abs() > 8 && dy.abs() > 8) {
                        effect = 255;
                    }
                    Hsv {
                        h: base.h.wrapping_add((dy as i32 / 4) as u8),
                        v: qadd8(hsv.v, 255 - effect as u8),
                        ..hsv
                    }
                }
                ReactiveShape::Splash | ReactiveShape::SolidSplash => {
                    let effect = tick.wrapping_sub(dist as u16).min(255) as u8;
                    let hue = if matches!(shape, ReactiveShape::Splash) {
                        hsv.h.wrapping_add(effect)
                    } else {
                        hsv.h
                    };
                    Hsv {
                        h: hue,
                        v: qadd8(hsv.v, 255 - effect),
                        ..hsv
                    }
                }
            };
        }
        hsv.v = scale8(hsv.v, base.v);
        hsv_to_rgb(hsv)
    });
}

// -------------------------------------------------------------- typing heatmap

/// How much heat a press adds to its own cell, QMK's
/// `RGB_MATRIX_TYPING_HEATMAP_INCREASE_STEP`.
const HEATMAP_INCREASE_STEP: u8 = 32;
/// How far a press spreads heat, in QMK's coordinate units.
const HEATMAP_SPREAD: u8 = 40;
/// Ceiling on the heat one neighbour can receive from a single press.
const HEATMAP_AREA_LIMIT: u8 = 16;
/// How often all heat is decreased by one, QMK's
/// `RGB_MATRIX_TYPING_HEATMAP_DECREASE_DELAY_MS`.
const HEATMAP_DECREASE_DELAY_MS: u32 = 25;

/// Add one key press's heat to the frame buffer, QMK's
/// `process_rgb_matrix_typing_heatmap`.
pub(crate) fn typing_heatmap_key(cfg: &LightingConfig, state: &mut EffectState, row: u8, col: u8) {
    // A key with no LED behind it heats nothing.
    let Some(source) = cfg.led_at(row, col) else {
        return;
    };
    let (source_x, source_y) = cfg.points[source];

    for cell_row in 0..cfg.matrix_rows {
        for cell_col in 0..cfg.matrix_cols {
            let Some(led) = cfg.led_at(cell_row, cell_col) else {
                continue;
            };
            let heat = &mut state.frame_buffer[cell_row as usize][cell_col as usize];
            if cell_row == row && cell_col == col {
                *heat = qadd8(*heat, HEATMAP_INCREASE_STEP);
                continue;
            }
            let (x, y) = cfg.points[led];
            let dx = x as i32 - source_x as i32;
            let dy = y as i32 - source_y as i32;
            let distance = sqrt16((dx * dx + dy * dy) as u16);
            if distance <= HEATMAP_SPREAD {
                *heat = qadd8(*heat, qsub8(HEATMAP_SPREAD, distance).min(HEATMAP_AREA_LIMIT));
            }
        }
    }
}

fn typing_heatmap(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    if ctx.init {
        ctx.state.frame_buffer = [[0; MAX_MATRIX_COLS]; MAX_MATRIX_ROWS];
        frame.fill(Rgb::BLACK);
    }

    // QMK updates this once per frame rather than once per chunk.
    let decrease = ctx.timer.wrapping_sub(ctx.state.heatmap_decrease_ms) >= HEATMAP_DECREASE_DELAY_MS;
    if decrease {
        ctx.state.heatmap_decrease_ms = ctx.timer;
    }

    for row in 0..ctx.cfg.matrix_rows {
        for col in 0..ctx.cfg.matrix_cols {
            let heat = ctx.state.frame_buffer[row as usize][col as usize];
            let Some(led) = ctx.cfg.led_at(row, col) else {
                continue;
            };
            if !ctx.led_enabled(led) {
                continue;
            }
            frame[led] = hsv_to_rgb(Hsv {
                h: 170 - qsub8(heat, 85),
                s: ctx.hsv.s,
                v: scale8(((qadd8(170, heat) as i32 - 170) * 3) as u8, ctx.hsv.v),
            });
            if decrease {
                ctx.state.frame_buffer[row as usize][col as usize] = qsub8(heat, 1);
            }
        }
    }
}

// --------------------------------------------------------------- digital rain

/// How often a column may start a new drop, as `1 / RGB_DIGITAL_RAIN_DROPS`.
const DIGITAL_RAIN_DROP_CHANCE: u8 = u8::MAX / 24;
/// Ticks between drops advancing, QMK's `drop_ticks`.
const DIGITAL_RAIN_DROP_TICKS: u8 = 28;

fn digital_rain(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let max_intensity = ctx.hsv.v;
    if ctx.init {
        ctx.state.frame_buffer = [[0; MAX_MATRIX_COLS]; MAX_MATRIX_ROWS];
        ctx.state.digital_rain_drop = 0;
        frame.fill(Rgb::BLACK);
    }

    // QMK divides by the configured brightness throughout this effect, and by
    // `pure_green_intensity` in particular, which is zero below a brightness of
    // four. There is nothing to show at that point, so stop before the divide.
    let pure_green = ((max_intensity as u16 * 3) >> 2) as u8;
    if pure_green == 0 {
        frame.fill(Rgb::BLACK);
        return;
    }
    let decay_ticks = u8::MAX / max_intensity;

    ctx.state.digital_rain_decay = ctx.state.digital_rain_decay.wrapping_add(1);
    let decay = ctx.state.digital_rain_decay;
    let drop = ctx.state.digital_rain_drop;

    for col in 0..ctx.cfg.matrix_cols {
        for row in 0..ctx.cfg.matrix_rows {
            let cell = &mut ctx.state.frame_buffer[row as usize][col as usize];
            if row == 0 && drop == 0 && ctx.state.rand.random8() < DIGITAL_RAIN_DROP_CHANCE {
                // The top row has just fallen, so a new drop starts here.
                *cell = max_intensity;
            } else if *cell > 0 && *cell < max_intensity && decay == decay_ticks {
                *cell -= 1;
            }
            let intensity = *cell;

            let Some(led) = ctx.cfg.led_at(row, col) else {
                continue;
            };
            // QMK draws this effect without consulting the flag mask.
            frame[led] = if intensity > pure_green {
                let boost =
                    (pure_green as u16 * (intensity - pure_green) as u16 / (max_intensity - pure_green) as u16) as u8;
                Rgb::new(boost, max_intensity, boost)
            } else {
                Rgb::new(
                    0,
                    (max_intensity as u16 * intensity as u16 / pure_green as u16) as u8,
                    0,
                )
            };
        }
    }

    if decay == decay_ticks {
        ctx.state.digital_rain_decay = 0;
    }
    ctx.state.digital_rain_drop = ctx.state.digital_rain_drop.wrapping_add(1);
    if ctx.state.digital_rain_drop > DIGITAL_RAIN_DROP_TICKS {
        ctx.state.digital_rain_drop = 0;
        for row in (1..ctx.cfg.matrix_rows).rev() {
            for col in 0..ctx.cfg.matrix_cols {
                let above = ctx.state.frame_buffer[(row - 1) as usize][col as usize];
                let current = ctx.state.frame_buffer[row as usize][col as usize];
                // A bright pixel on the bottom row starts decaying as the rain
                // falls past it.
                let mut next = if row == ctx.cfg.matrix_rows - 1 && current == max_intensity {
                    current - 1
                } else {
                    current
                };
                if above >= max_intensity {
                    // Let the old bright pixel decay and light this one.
                    ctx.state.frame_buffer[(row - 1) as usize][col as usize] = max_intensity - 1;
                    next = max_intensity;
                }
                ctx.state.frame_buffer[row as usize][col as usize] = next;
            }
        }
    }
}

// -------------------------------------------------------------- pixel fractal

fn pixel_fractal(ctx: &mut EffectCtx, frame: &mut [Rgb]) {
    let rows = ctx.cfg.matrix_rows;
    let cols = ctx.cfg.matrix_cols;
    // QMK lights the left half and mirrors it onto the right.
    let mid_col = if cols < 2 { 1 } else { cols / 2 };

    if ctx.init {
        ctx.state.fractal = [[false; MAX_MATRIX_COLS]; MAX_MATRIX_ROWS];
        frame.fill(Rgb::BLACK);
    }

    if ctx.timer > ctx.state.fractal_wait_timer {
        let colour = hsv_to_rgb(ctx.hsv);
        for row in 0..rows {
            for col in 0..mid_col {
                let rgb = if ctx.state.fractal[row as usize][col as usize] {
                    colour
                } else {
                    Rgb::BLACK
                };
                let left = ctx.cfg.led_at(row, col);
                let right = ctx.cfg.led_at(row, cols - 1 - col);
                for led in [left, right].into_iter().flatten() {
                    if ctx.led_enabled(led) {
                        frame[led] = rgb;
                    }
                }
            }
        }

        for row in 0..rows {
            for col in 0..(mid_col as usize).saturating_sub(1) {
                ctx.state.fractal[row as usize][col] = ctx.state.fractal[row as usize][col + 1];
            }
            ctx.state.fractal[row as usize][mid_col as usize - 1] = ctx.state.rand.random8() & 3 == 0;
        }
        let interval = 3000 / scale16by8(qadd8(ctx.speed, 16) as u16, 16) as u32;
        ctx.state.fractal_wait_timer = ctx.timer + interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two LEDs, one matrix row, far enough apart that a spread effect cannot
    /// reach from one to the other.
    static POINTS: [(u8, u8); 2] = [(0, 32), (224, 32)];
    static FLAGS: [u8; 2] = [4, 4];
    static MATRIX: [Option<(u8, u8)>; 2] = [Some((0, 0)), Some((0, 1))];
    static MODES: [u16; 2] = [2, 31];
    static CFG: LightingConfig = LightingConfig {
        max_brightness: 255,
        frame_ms: 16,
        timeout_ms: 0,
        center: (112, 32),
        hue_steps: 8,
        sat_steps: 16,
        val_steps: 16,
        speed_steps: 16,
        react_on_keyup: false,
        default_on: true,
        default_mode: 2,
        default_hue: 0,
        default_sat: 255,
        default_val: 255,
        default_speed: 128,
        default_flags: 255,
        modes: &MODES,
        vial_modes: &MODES,
        points: &POINTS,
        flags: &FLAGS,
        matrix: &MATRIX,
        matrix_rows: 1,
        matrix_cols: 2,
    };

    fn build_ctx<'a>(state: &'a mut EffectState, hsv: Hsv, init: bool) -> EffectCtx<'a> {
        EffectCtx {
            cfg: &CFG,
            hsv,
            speed: 128,
            flags: 255,
            timer: 1000,
            init,
            state,
        }
    }

    #[test]
    fn solid_colour_paints_every_led() {
        let mut state = EffectState::new();
        let mut frame = [Rgb::BLACK; 2];
        let mut ctx = build_ctx(&mut state, Hsv::new(0, 255, 255), true);
        render(Effect::SolidColor, &mut ctx, &mut frame);
        assert_eq!(frame, [Rgb::new(255, 0, 0), Rgb::new(255, 0, 0)]);
    }

    #[test]
    fn a_reactive_hit_lights_its_led_and_then_fades() {
        let mut state = EffectState::new();
        state.hits.record(0, POINTS[0].0, POINTS[0].1);
        let mut frame = [Rgb::BLACK; 2];
        let mut ctx = build_ctx(&mut state, Hsv::new(0, 255, 255), false);
        render(Effect::SolidReactiveSimple, &mut ctx, &mut frame);
        assert_eq!(frame[0], Rgb::new(255, 0, 0), "the hit LED is at full brightness");
        assert_eq!(frame[1], Rgb::BLACK, "the LED that was not hit stays dark");

        // Three seconds later the same hit has faded out completely.
        let mut aged = EffectState::new();
        aged.hits.record(0, POINTS[0].0, POINTS[0].1);
        aged.hits.age(3000);
        let mut frame = [Rgb::BLACK; 2];
        let mut ctx = build_ctx(&mut aged, Hsv::new(0, 255, 255), false);
        render(Effect::SolidReactiveSimple, &mut ctx, &mut frame);
        assert_eq!(frame, [Rgb::BLACK; 2]);
    }

    #[test]
    fn a_press_heats_its_key_and_renders() {
        let mut state = EffectState::new();
        typing_heatmap_key(&CFG, &mut state, 0, 0);
        assert_eq!(state.frame_buffer[0][0], HEATMAP_INCREASE_STEP);
        assert_eq!(
            state.frame_buffer[0][1], 0,
            "224 units away is outside the 40-unit spread"
        );

        let mut frame = [Rgb::BLACK; 2];
        // Not an init frame: the heat was added between frames, which is when
        // the firmware feeds key events in.
        let mut ctx = build_ctx(&mut state, Hsv::new(0, 255, 128), false);
        render(Effect::TypingHeatmap, &mut ctx, &mut frame);
        assert_ne!(frame[0], Rgb::BLACK, "a hot key lights up");
        assert_eq!(frame[1], Rgb::BLACK);
    }

    #[test]
    fn digital_rain_survives_a_brightness_of_zero() {
        let mut state = EffectState::new();
        let mut frame = [Rgb::new(9, 9, 9); 2];
        // QMK divides by the brightness here, so zero would be a division by
        // zero rather than a dark chain.
        let mut ctx = build_ctx(&mut state, Hsv::new(0, 255, 0), true);
        render(Effect::DigitalRain, &mut ctx, &mut frame);
        assert_eq!(frame, [Rgb::BLACK; 2]);
    }

    #[test]
    fn pixel_fractal_mirrors_one_half_onto_the_other() {
        let mut state = EffectState::new();
        let mut frame = [Rgb::BLACK; 2];
        // Force the left half lit, then let the effect draw it.
        state.fractal[0][0] = true;
        let mut ctx = build_ctx(&mut state, Hsv::new(0, 255, 255), false);
        // The pattern is only redrawn once the wait timer allows it.
        ctx.state.fractal_wait_timer = 0;
        render(Effect::PixelFractal, &mut ctx, &mut frame);
        assert_eq!(frame[0], Rgb::new(255, 0, 0), "the lit left half is drawn");
        assert_eq!(frame[1], Rgb::new(255, 0, 0), "and mirrored onto the right half");
    }
}
